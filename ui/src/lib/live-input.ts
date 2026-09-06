// Field-local edit groups: a focused edit can update live, persist after a short
// idle, and still form one undo transaction. State is not kept on the document.
export function inputGroup(element: HTMLElement): string {
  element.dataset.liveGroup ??= crypto.randomUUID();
  return element.dataset.liveGroup;
}
export function releaseInputGroup(element: HTMLElement) {
  delete element.dataset.liveGroup;
}
export function finiteInput(raw: string): number | null {
  if (!raw.trim() || !Number.isFinite(Number(raw))) return null;
  return Number(raw);
}
