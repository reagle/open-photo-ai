/**
 * What is open over the photograph and has an Escape - and keys - of its own: a dialog or alert dialog (a popover's
 * panel is one), a menu or a listbox, and not one that is closing.
 *
 * What the canvas's own key handling stands aside for: a key meant for a surface drawn over the photograph is not one
 * for the photograph.
 */
export const OPEN_LAYER = ['[role="dialog"]', '[role="alertdialog"]', '[role="menu"]', '[role="listbox"]']
    .map((layer) => `${layer}:not([data-state="closed"])`)
    .join(",");
