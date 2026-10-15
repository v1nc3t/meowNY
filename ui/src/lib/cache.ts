/**
 * Small caches for things that are slow to build and never change for the same inputs:
 * `Intl` formatters and the option lists made from them. Building a formatter costs far more
 * than using it, and lists are redrawn on every keystroke without this.
 */
export function memoByKey<A extends unknown[], R>(key: (...args: A) => string, make: (...args: A) => R): (...args: A) => R {
  const store = new Map<string, R>()
  return (...args: A) => {
    const k = key(...args)
    if (!store.has(k)) store.set(k, make(...args))
    return store.get(k) as R
  }
}
