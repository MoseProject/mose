// Applies the backend's partial session updates (`session-patch`) to the project JSON the webview
// holds. Only the containers along a changed path are copied, so everything that did not change
// keeps its identity and React can skip it.

export interface PatchOp {
  /** Where: object keys and array indexes from the project root. */
  p: (string | number)[];
  /** The new value at `p`; an index equal to the array's length appends. */
  v?: unknown;
  /** Instead of `v`: cut the array at `p` to this length. */
  len?: number;
}

type Container = Record<string | number, unknown> | unknown[];

function copyOf(node: unknown): Container {
  return Array.isArray(node) ? node.slice() : { ...(node as Record<string, unknown>) };
}

/** `root` with `ops` applied; `root` itself is not modified. */
export function applyOps<T>(root: T, ops: PatchOp[]): T {
  // Containers copied during this call can be changed in place by the ops after the first.
  const owned = new WeakSet<object>();
  const own = (node: unknown): Container => {
    if (owned.has(node as object)) {
      return node as Container;
    }
    const copy = copyOf(node);
    owned.add(copy);
    return copy;
  };
  const result = own(root) as Container;
  for (const op of ops) {
    if (op.p.length === 0) {
      continue;
    }
    // Walk down, replacing each container on the way by its (owned) copy.
    let parent = result;
    for (let depth = 0; depth < op.p.length - 1; depth += 1) {
      const key = op.p[depth];
      const next = own((parent as Record<string | number, unknown>)[key]);
      (parent as Record<string | number, unknown>)[key] = next;
      parent = next;
    }
    const last = op.p[op.p.length - 1];
    if (op.len !== undefined) {
      const target = own((parent as Record<string | number, unknown>)[last]) as unknown[];
      target.length = op.len;
      (parent as Record<string | number, unknown>)[last] = target;
    } else {
      (parent as Record<string | number, unknown>)[last] = op.v;
    }
  }
  return result as unknown as T;
}
