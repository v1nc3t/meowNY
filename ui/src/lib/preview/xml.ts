import { PreviewError } from './errors'

/** An element of a document read by `parseXml`: its name without the prefix, its attributes without theirs, and what it holds. */
export type XmlNode = { name: string; attrs: Record<string, string>; kids: (XmlNode | string)[] }

/** Most elements one part may have; a file that has more is not a letter or a sheet but an attempt on the reader. */
const MAX_NODES = 2_000_000
const MAX_DEPTH = 100

const ENTITIES: Record<string, string> = { lt: '<', gt: '>', amp: '&', quot: '"', apos: "'" }

function decode(text: string): string {
  if (!text.includes('&')) return text
  return text.replace(/&(#x[0-9a-fA-F]{1,6}|#[0-9]{1,7}|[a-z]{2,4});/g, (whole, body: string) => {
    if (body.startsWith('#')) {
      const code = body[1] === 'x' ? parseInt(body.slice(2), 16) : parseInt(body.slice(1), 10)
      return code > 0 && code <= 0x10ffff && !(code >= 0xd800 && code <= 0xdfff) ? String.fromCodePoint(code) : ''
    }
    return ENTITIES[body] ?? whole
  })
}

function local(name: string): string {
  const colon = name.indexOf(':')
  return colon < 0 ? name : name.slice(colon + 1)
}

const ATTRIBUTE = /([^\s=/>]+)\s*=\s*(?:"([^"]*)"|'([^']*)')/g

function attributes(text: string): Record<string, string> {
  const attrs: Record<string, string> = {}
  for (const match of text.matchAll(ATTRIBUTE)) {
    const name = match[1] ?? ''
    if (name === 'xmlns' || name.startsWith('xmlns:')) continue
    attrs[local(name)] = decode(match[2] ?? match[3] ?? '')
  }
  return attrs
}

/**
 * Reads the XML of an Office file into plain objects. It is small on purpose, so that it can be said what it does not do: no
 * namespaces (names are matched without their prefix), no document type and therefore no entities of its own (a file that has one
 * is refused, which is where the "billion laughs" tricks live), only the five named entities and number references. Nothing is
 * fetched, nothing is run. The result is checked to be one tree, closed properly.
 */
export function parseXml(source: string): XmlNode {
  const root: XmlNode = { name: '', attrs: {}, kids: [] }
  const stack: XmlNode[] = [root]
  let nodes = 0
  let at = 0
  while (at < source.length) {
    const open = source.indexOf('<', at)
    const top = stack[stack.length - 1] as XmlNode
    if (open < 0) {
      pushText(top, source.slice(at))
      break
    }
    if (open > at) pushText(top, source.slice(at, open))
    if (source.startsWith('<!--', open)) {
      const end = source.indexOf('-->', open + 4)
      if (end < 0) throw new PreviewError('unreadable')
      at = end + 3
    } else if (source.startsWith('<![CDATA[', open)) {
      const end = source.indexOf(']]>', open + 9)
      if (end < 0) throw new PreviewError('unreadable')
      top.kids.push(source.slice(open + 9, end))
      at = end + 3
    } else if (source.startsWith('<?', open)) {
      const end = source.indexOf('?>', open + 2)
      if (end < 0) throw new PreviewError('unreadable')
      at = end + 2
    } else if (source.startsWith('<!', open)) {
      throw new PreviewError('unsupported')
    } else if (source.startsWith('</', open)) {
      const end = source.indexOf('>', open + 2)
      if (end < 0 || stack.length < 2) throw new PreviewError('unreadable')
      if (local(source.slice(open + 2, end).trim()) !== top.name) throw new PreviewError('unreadable')
      stack.pop()
      at = end + 1
    } else {
      const end = tagEnd(source, open + 1)
      if (end < 0) throw new PreviewError('unreadable')
      const selfClosed = source[end - 1] === '/'
      const inside = source.slice(open + 1, selfClosed ? end - 1 : end)
      const space = inside.search(/\s/)
      const name = local(space < 0 ? inside : inside.slice(0, space))
      if (name === '') throw new PreviewError('unreadable')
      nodes += 1
      if (nodes > MAX_NODES) throw new PreviewError('tooBig')
      const node: XmlNode = { name, attrs: space < 0 ? {} : attributes(inside.slice(space)), kids: [] }
      top.kids.push(node)
      if (!selfClosed) {
        if (stack.length > MAX_DEPTH) throw new PreviewError('tooBig')
        stack.push(node)
      }
      at = end + 1
    }
  }
  const only = root.kids.find((kid): kid is XmlNode => typeof kid !== 'string')
  if (stack.length !== 1 || !only) throw new PreviewError('unreadable')
  return only
}

/** Where the `>` that ends a tag is, skipping one inside a quoted value; -1 if the tag never ends. */
function tagEnd(source: string, from: number): number {
  let quote = ''
  for (let at = from; at < source.length; at += 1) {
    const char = source[at]
    if (quote) {
      if (char === quote) quote = ''
    } else if (char === '"' || char === "'") {
      quote = char
    } else if (char === '>') {
      return at
    }
  }
  return -1
}

/** Text between elements is kept when it holds something, or a space inside a line; indentation between elements is dropped. */
function pushText(node: XmlNode, text: string): void {
  if (text.trim() === '' && (text.includes('\n') || text.includes('\r'))) return
  if (node.name === '') return
  node.kids.push(decode(text))
}

/** The elements inside one, in order. */
export function children(node: XmlNode, name?: string): XmlNode[] {
  return node.kids.filter((kid): kid is XmlNode => typeof kid !== 'string' && (name === undefined || kid.name === name))
}

/** The first element of that name inside one. */
export function child(node: XmlNode, name: string): XmlNode | undefined {
  return children(node, name)[0]
}

/** The text an element holds directly. */
export function textOf(node: XmlNode): string {
  return node.kids.filter((kid): kid is string => typeof kid === 'string').join('')
}
