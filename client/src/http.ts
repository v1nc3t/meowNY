export function api(path: string, init?: RequestInit): Promise<Response> {
  const headers = new Headers(init?.headers)
  if (typeof init?.body === 'string' && !headers.has('content-type')) {
    headers.set('content-type', 'application/json')
  }
  return fetch(path, { ...init, credentials: 'include', headers })
}

export async function errorMessage(response: Response): Promise<string> {
  try {
    const body = (await response.json()) as { error?: unknown; fields?: unknown }
    const error = typeof body.error === 'string' ? body.error : ''
    const fields = Array.isArray(body.fields)
      ? body.fields.filter((field): field is string => typeof field === 'string')
      : []
    if (error && fields.length > 0) return `${error}: ${fields.join(', ')}`
    if (error) return error
  } catch {
    // The body was empty or not JSON.
  }
  return 'request failed.'
}
