// Names for new projects: two or three words in the spirit of "Mega Studio" or "Imax Giga
// Master", so a new project starts with a name of its own instead of "Untitled".

import { localeSnapshot } from "./i18n";

interface Lists {
  /** Intensifiers and brands that go in front. */
  boosters: string[];
  /** The thing being made. */
  things: string[];
  /** Endings that give it a title. */
  titles: string[];
}

type NameLocale = "pt-BR" | "en";

const LISTS: Record<NameLocale, Lists> = {
  en: {
    boosters: [
      "Mega",
      "Giga",
      "Ultra",
      "Hyper",
      "Turbo",
      "Imax",
      "Super",
      "Max",
      "Omega",
      "Quantum",
      "Atomic",
      "Cosmic",
      "Epic",
      "Nitro",
      "Titan",
      "Prime",
    ],
    things: [
      "Studio",
      "Cut",
      "Reel",
      "Montage",
      "Blockbuster",
      "Director",
      "Clip",
      "Trailer",
      "Remix",
      "Timeline",
      "Highlights",
      "Cinema",
    ],
    titles: [
      "Master",
      "Deluxe",
      "Pro",
      "Plus",
      "Supreme",
      "Edition",
      "Legend",
      "Ultimate",
      "Wizard",
      "Champion",
      "Maximus",
      "Extreme",
    ],
  },
  "pt-BR": {
    boosters: [
      "Mega",
      "Giga",
      "Ultra",
      "Hiper",
      "Turbo",
      "Imax",
      "Super",
      "Máximo",
      "Ômega",
      "Quântico",
      "Atômico",
      "Cósmico",
      "Épico",
      "Nitro",
      "Titã",
      "Prime",
    ],
    things: [
      "Estúdio",
      "Corte",
      "Cinema",
      "Montagem",
      "Superprodução",
      "Diretor",
      "Clipe",
      "Trailer",
      "Remix",
      "Edição",
      "Melhores Momentos",
      "Filmão",
    ],
    titles: [
      "Mestre",
      "Deluxe",
      "Pro",
      "Plus",
      "Supremo",
      "Master",
      "Lenda",
      "Definitivo",
      "Mago",
      "Campeão",
      "Maximus",
      "Extremo",
    ],
  },
};

const pick = <T>(items: T[], random: () => number): T => {
  return items[Math.min(items.length - 1, Math.floor(random() * items.length))];
};

const currentLocale = (): NameLocale => {
  return localeSnapshot() === "en" ? "en" : "pt-BR";
};

/** A funny project name such as "Mega Studio" or "Imax Giga Master": a booster and a thing, and
 *  now and then a second booster or a title. `random` is any source of numbers in [0, 1). */
export function funnyProjectName(
  random: () => number = Math.random,
  locale: NameLocale = currentLocale(),
): string {
  const { boosters, things, titles } = LISTS[locale];
  const first = pick(boosters, random);
  const shape = random();
  if (shape < 0.34) {
    return first + " " + pick(things, random);
  }
  if (shape < 0.67) {
    return first + " " + pick(things, random) + " " + pick(titles, random);
  }
  // A second, different booster in front of the title: "Imax Giga Master".
  let second = pick(boosters, random);
  if (second === first) {
    second = boosters[(boosters.indexOf(first) + 1) % boosters.length];
  }
  return first + " " + second + " " + pick(titles, random);
}

/** A funny name that none of `taken` already has (compared without regard to case), so a new
 *  project can be told apart from the ones already on the list. */
export function uniqueFunnyName(
  taken: string[],
  random: () => number = Math.random,
  locale: NameLocale = currentLocale(),
): string {
  const used = new Set(
    taken.map((name) => {
      return name.trim().toLowerCase();
    }),
  );
  let name = funnyProjectName(random, locale);
  for (let attempt = 0; attempt < 50 && used.has(name.toLowerCase()); attempt += 1) {
    name = funnyProjectName(random, locale);
  }
  // Fifty draws out of thousands of combinations only fail when nearly all are taken; a number
  // still keeps the name unique.
  const base = name;
  for (let n = 2; used.has(name.toLowerCase()); n += 1) {
    name = base + " " + n;
  }
  return name;
}
