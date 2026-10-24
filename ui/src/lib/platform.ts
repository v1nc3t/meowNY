/** Whether this is the iPhone (or iPad) app. An iPad's web view says Macintosh, but a Mac has no touch screen. */
export const isIos =
  typeof navigator !== 'undefined' && (/iPhone|iPad|iPod/.test(navigator.userAgent) || (/Macintosh/.test(navigator.userAgent) && navigator.maxTouchPoints > 1))
