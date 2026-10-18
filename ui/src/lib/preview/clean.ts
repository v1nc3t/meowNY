/**
 * Text from a file, made fit to show. A control character, a mark that reverses the direction of what follows and a zero-width
 * character can make a line read one way and be another, and none of them has a use in what the preview shows. A tab and a line
 * break stay. (The app refuses the same characters in the name of a document.)
 */
export function clean(text: string): string {
  // eslint-disable-next-line no-control-regex
  return text.replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u009f­؜᠎​-‏‪-‮⁠-⁤⁦-⁯﻿]/g, '')
}

/** The most characters of one text the preview draws; the rest is said to be left out, so a huge file cannot freeze the window. */
export const MAX_TEXT_CHARS = 400_000

/** Bytes as text for showing: UTF-8, what cannot be read shown as a replacement mark, cleaned, and cut at the limit. */
export function textOfBytes(bytes: Uint8Array): { text: string; cut: boolean } {
  const whole = clean(new TextDecoder('utf-8', { fatal: false }).decode(bytes))
  return whole.length > MAX_TEXT_CHARS ? { text: whole.slice(0, MAX_TEXT_CHARS), cut: true } : { text: whole, cut: false }
}
