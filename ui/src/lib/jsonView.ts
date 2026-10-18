/** One piece of a JSON text, for colouring: a key, a string, a number, true, false or null, or the rest (brackets, commas, spaces). */
export type JsonToken = { kind: 'key' | 'string' | 'number' | 'literal' | 'plain'; text: string }

const STRING = /"(?:[^"\\]|\\.)*"?/y
const COLON = /\s*:/y
const NUMBER = /-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/y
const LITERAL = /(?:true|false|null)(?![A-Za-z0-9_])/y

function at(pattern: RegExp, text: string, index: number): string | null {
  pattern.lastIndex = index
  const match = pattern.exec(text)
  return match ? match[0] : null
}

/**
 * Cuts a JSON text into pieces to colour. It does not check the JSON: it is for showing the start of a file that may be cut in the
 * middle of a string, and a text that is not JSON simply comes out as plain. The pieces put back together are the text.
 */
export function tokenizeJson(text: string): JsonToken[] {
  const tokens: JsonToken[] = []
  const push = (kind: JsonToken['kind'], piece: string) => {
    const last = tokens[tokens.length - 1]
    if (last && last.kind === kind && kind === 'plain') last.text += piece
    else tokens.push({ kind, text: piece })
  }
  let index = 0
  while (index < text.length) {
    const char = text[index] as string
    const string = char === '"' ? at(STRING, text, index) : null
    if (string !== null) {
      // a string followed by a colon is the name of a field
      push(at(COLON, text, index + string.length) !== null ? 'key' : 'string', string)
      index += string.length
      continue
    }
    const number = char === '-' || (char >= '0' && char <= '9') ? at(NUMBER, text, index) : null
    if (number !== null) {
      push('number', number)
      index += number.length
      continue
    }
    const literal = char === 't' || char === 'f' || char === 'n' ? at(LITERAL, text, index) : null
    if (literal !== null) {
      push('literal', literal)
      index += literal.length
      continue
    }
    push('plain', char)
    index += 1
  }
  return tokens
}
