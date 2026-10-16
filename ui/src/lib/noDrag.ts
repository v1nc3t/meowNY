/**
 * Selected text can be picked up with the mouse and dragged. Nothing in the app takes a drop, and in the desktop window the
 * drag paints a trail of the cursor over the page, so a drag is not allowed to start. Selecting text with the mouse is not a
 * drag and still works, and so does dropping a file on a file field.
 */
export function installNoDragHandler(): void {
  document.addEventListener('dragstart', (event) => event.preventDefault())
}
