// Immutable clip arrays share an interval tree. Painting visits only intersecting subtrees;
// results retain original array order so overlapping clips preserve their painter order.
interface Interval {
  start_secs: number;
}
interface Node<T> {
  item: T;
  end: number;
  maxEnd: number;
  order: number;
  left?: Node<T>;
  right?: Node<T>;
}
const indexes = new WeakMap<readonly object[], Node<Interval> | undefined>();

export function visibleClips<T extends Interval>(
  clips: readonly T[],
  from: number,
  to: number,
  duration: (clip: T) => number,
): T[] {
  let root = indexes.get(clips) as Node<T> | undefined;
  if (!indexes.has(clips)) {
    const entries = clips.map((item, order) => {
      return { item, order, end: item.start_secs + duration(item) };
    })
      .sort((a, b) => {
        return a.item.start_secs - b.item.start_secs || a.order - b.order;
      });
    const build = (lo: number, hi: number): Node<T> | undefined => {
      if (lo >= hi) {
        return undefined;
      }
      const mid = (lo + hi) >>> 1;
      const left = build(lo, mid);
      const right = build(mid + 1, hi);
      const entry = entries[mid];
      return {
        ...entry,
        left,
        right,
        maxEnd: Math.max(entry.end, left?.maxEnd ?? -Infinity, right?.maxEnd ?? -Infinity),
      };
    };
    root = build(0, entries.length);
    indexes.set(clips, root as Node<Interval> | undefined);
  }
  const found: Node<T>[] = [];
  const visit = (node?: Node<T>) => {
    if (!node || node.maxEnd < from) {
      return;
    }
    visit(node.left);
    if (node.item.start_secs > to) {
      return;
    }
    if (node.end >= from) {
      found.push(node);
    }
    visit(node.right);
  };
  visit(root);
  return found.sort((a, b) => {
    return a.order - b.order;
  }).map(node => {
    return node.item;
  });
}
