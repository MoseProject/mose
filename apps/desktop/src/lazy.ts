import { lazy, type ComponentType, type LazyExoticComponent } from "react";

// A component type with any props: the lazy component keeps the real props of `M[K]`.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyComponent = ComponentType<any>;

/** The names a module exports a component under. */
type ComponentNames<M> = {
  [Name in keyof M]: M[Name] extends AnyComponent ? Name : never;
}[keyof M] &
  string;

/** `React.lazy` for a module that exports its component by name: the module is fetched the first
 *  time the component is rendered, which is what keeps what is not on screen out of the first
 *  load. */
export function lazyNamed<M extends object, K extends ComponentNames<M>>(
  load: () => Promise<M>,
  name: K,
): LazyExoticComponent<Extract<M[K], AnyComponent>> {
  return lazy(async () => {
    const module = await load();
    return { default: module[name] as Extract<M[K], AnyComponent> };
  });
}
