/** Why a file could not be shown: it is not what its name says, it is cut or damaged, or it is bigger than the preview reads. */
export class PreviewError extends Error {
  readonly reason: 'unreadable' | 'tooBig' | 'unsupported'

  constructor(reason: 'unreadable' | 'tooBig' | 'unsupported') {
    super(reason)
    this.name = 'PreviewError'
    this.reason = reason
  }
}
