// @vitest-environment jsdom
import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useDraft } from "../src/hooks/useDraft";

describe("useDraft", () => {
  it("starts from the source", () => {
    const { result } = renderHook(() => {
      return useDraft("a");
    });
    expect(result.current[0]).toBe("a");
  });

  it("keeps a local edit while the source is unchanged", () => {
    const { result, rerender } = renderHook(
      ({ source }) => {
        return useDraft(source);
      },
      {
        initialProps: { source: "a" },
      },
    );
    act(() => {
      result.current[1]("typed");
    });
    expect(result.current[0]).toBe("typed");
    rerender({ source: "a" });
    expect(result.current[0]).toBe("typed");
  });

  it("follows the source when it changes, dropping the edit", () => {
    const { result, rerender } = renderHook(
      ({ source }) => {
        return useDraft(source);
      },
      {
        initialProps: { source: "a" },
      },
    );
    act(() => {
      result.current[1]("typed");
    });
    rerender({ source: "b" });
    expect(result.current[0]).toBe("b");
    act(() => {
      result.current[1]("again");
    });
    expect(result.current[0]).toBe("again");
  });

  it("treats a new array identity as a new source", () => {
    const first = [1];
    const { result, rerender } = renderHook(
      ({ source }) => {
        return useDraft(source);
      },
      {
        initialProps: { source: first },
      },
    );
    act(() => {
      result.current[1]([1, 2]);
    });
    rerender({ source: first });
    expect(result.current[0]).toEqual([1, 2]);
    rerender({ source: [9] });
    expect(result.current[0]).toEqual([9]);
  });
});
