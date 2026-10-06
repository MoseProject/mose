type ClassValue = string | false | null | undefined;
type CssModule = Readonly<Record<string, string>>;

/** Resolve class tokens through the CSS Modules used by a component area and shared base. */
export function cx(...values: (CssModule | ClassValue)[]): string {
  const modules = values.filter((value): value is CssModule => {
    return typeof value === "object" && value !== null;
  });
  const names = values.flatMap((value) => {
    return typeof value === "string" ? value.trim().split(/\s+/).filter(Boolean) : [];
  });
  return [...new Set(names)]
    .flatMap((name) => {
      const scoped = modules.map((module) => module[name]).filter(Boolean);
      return scoped.length ? [...new Set(scoped), name] : [name];
    })
    .join(" ");
}
