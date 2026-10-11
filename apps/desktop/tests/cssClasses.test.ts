// Guards the CSS Modules wiring that unit tests cannot see (jsdom applies no stylesheet).
//
// `cx(styles, base, "a b")` turns each token into the hashed class of the modules it is given and
// always keeps the plain name too. A token that none of those modules defines still renders, it is
// just unstyled, with no error anywhere. That is how, after the styles were split into modules,
// every dialog lost its panel, the top bar lost its spacer and the page lost its height.
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { describe, expect, it } from "vitest";

const SRC = resolve(__dirname, "../src");

function walk(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    return entry.isDirectory() ? walk(path) : [path];
  });
}

const files = walk(SRC);

function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

/** Classes each module defines, and the ones written as `:global(.x)`, which match everywhere. */
const classesOf = new Map<string, Set<string>>();
const globalClasses = new Set<string>();
for (const file of files.filter((f) => {
  return f.endsWith(".css");
})) {
  const css = stripComments(readFileSync(file, "utf8"));
  const set = new Set<string>();
  for (const match of css.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)) {
    set.add(match[1]);
  }
  for (const match of css.matchAll(/:global\(\.(-?[_a-zA-Z][\w-]*)\)/g)) {
    globalClasses.add(match[1]);
  }
  classesOf.set(resolve(file), set);
}

/**
 * Classes that no stylesheet defines, on purpose: they are hooks for tests and automation, or
 * come from data. Each one is checked below to still be unstyled, so the list cannot rot.
 */
const UNSTYLED_HOOKS = [
  "Audio", // an asset's kind used as a class
  "bar", // Menu's variant marker
  "clip-checks",
  "field-row",
  "idle",
  "multi-selection",
  "number-field",
  "task-label",
];

interface Usage {
  where: string;
  token: string;
  modules: string[];
}

function usages(): Usage[] {
  const found: Usage[] = [];
  for (const file of files.filter((f) => {
    return /\.tsx?$/.test(f) && !/\.(test|stories)\./.test(f);
  })) {
    const source = readFileSync(file, "utf8");
    const imports = new Map<string, string>();
    for (const match of source.matchAll(/import\s+(\w+)\s+from\s+"([^"]+\.module\.css)"/g)) {
      imports.set(match[1], resolve(dirname(file), match[2]));
    }
    let at = 0;
    while ((at = source.indexOf("cx(", at)) !== -1) {
      if (at > 0 && /[\w.]/.test(source[at - 1])) {
        at += 3;
        continue;
      }
      let depth = 0;
      let end = -1;
      for (let i = at + 2; i < source.length; i++) {
        if (source[i] === "(") {
          depth++;
        } else if (source[i] === ")") {
          depth--;
          if (depth === 0) {
            end = i;
            break;
          }
        }
      }
      const call = source.slice(at + 3, end);
      const line = source.slice(0, at).split("\n").length;
      const modules = call
        .split(",")
        .map((part) => {
          return imports.get(part.trim());
        })
        .filter((m): m is string => {
          return m !== undefined;
        });
      for (const literal of call.matchAll(/"([^"$]*)"/g)) {
        for (const token of literal[1].split(/\s+/).filter(Boolean)) {
          found.push({ where: `${relative(SRC, file)}:${line}`, token, modules });
        }
      }
      at = end;
    }
  }
  return found;
}

function isDefined({ token, modules }: Usage): boolean {
  return (
    globalClasses.has(token) ||
    modules.some((m) => {
      return classesOf.get(m)?.has(token);
    })
  );
}

describe("CSS module class names", () => {
  const all = usages();

  it("finds the cx() calls it is meant to check", () => {
    expect(all.length).toBeGreaterThan(300);
  });

  it("resolves every class passed to cx() in a module that call passes, or in :global()", () => {
    const unresolved = all
      .filter((usage) => {
        return !isDefined(usage) && !UNSTYLED_HOOKS.includes(usage.token);
      })
      .map((usage) => {
        return `${usage.where} "${usage.token}" (modules: ${usage.modules
          .map((m) => {
            return relative(SRC, m);
          })
          .join(", ")})`;
      });
    expect(unresolved).toEqual([]);
  });

  it("keeps the list of intentionally unstyled classes honest", () => {
    const styledNow = UNSTYLED_HOOKS.filter((token) => {
      return all.some((usage) => {
        return usage.token === token && isDefined(usage);
      });
    });
    expect(styledNow, "these now have a style: take them off UNSTYLED_HOOKS").toEqual([]);
    const unused = UNSTYLED_HOOKS.filter((token) => {
      return !all.some((usage) => {
        return usage.token === token;
      });
    });
    expect(unused, "these are no longer used: take them off UNSTYLED_HOOKS").toEqual([]);
  });

  it("writes no id selector outside :global(), because CSS Modules hash ids and they stop matching", () => {
    const offenders: string[] = [];
    for (const file of files.filter((f) => {
      return f.endsWith(".module.css");
    })) {
      const css = stripComments(readFileSync(file, "utf8"));
      for (const rule of css.matchAll(/([^{}@;]+)\{/g)) {
        const selector = rule[1].replace(/:global\([^)]*\)/g, "");
        if (/(^|[\s>+~,])#[a-zA-Z_]/.test(selector)) {
          offenders.push(`${relative(SRC, file)}: ${rule[1].trim()}`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });
});
