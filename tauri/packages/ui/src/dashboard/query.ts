/**
 * Provider query/mutation helpers — thin wrappers around SolidJS primitives.
 *
 * createProviderQuery: wraps createResource for read operations.
 * createProviderMutation: wraps signals for write operations with loading/error state.
 *
 * No external dependencies — only SolidJS core.
 */

import type { ResourceReturn } from "solid-js";
import { createResource, createSignal } from "solid-js";

/**
 * Wrap a DataProvider read operation in createResource.
 *
 * Returns a standard SolidJS ResourceReturn — the accessor is reactive
 * and suspends during loading. Refetching is available via the returned
 * actions object.
 *
 * Usage:
 *   const [data, { refetch }] = createProviderQuery(() => provider.listConnectors());
 */
export function createProviderQuery<T>(fetcher: () => Promise<T>): ResourceReturn<T> {
  return createResource<T>(() => fetcher());
}

/**
 * Result of createProviderMutation.
 */
export interface MutationResult<TArgs extends unknown[], TResult> {
  /** Execute the mutation. */
  mutate: (...args: TArgs) => Promise<TResult | undefined>;
  /** Whether the mutation is currently in flight. */
  loading: () => boolean;
  /** Error message if the last mutation failed, null otherwise. */
  error: () => string | null;
  /** Reset error state. */
  reset: () => void;
}

/**
 * Wrap a DataProvider write operation with loading + error signals.
 *
 * Usage:
 *   const { mutate, loading, error } = createProviderMutation(
 *     (id: string) => provider.deployFormation(id)
 *   );
 *   await mutate("formation-123");
 */
export function createProviderMutation<TArgs extends unknown[], TResult>(
  fn: (...args: TArgs) => Promise<TResult>,
): MutationResult<TArgs, TResult> {
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const mutate = async (...args: TArgs): Promise<TResult | undefined> => {
    setLoading(true);
    setError(null);
    try {
      const result = await fn(...args);
      return result;
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      setError(message);
      return undefined;
    } finally {
      setLoading(false);
    }
  };

  const reset = () => {
    setError(null);
    setLoading(false);
  };

  return { mutate, loading, error, reset };
}
