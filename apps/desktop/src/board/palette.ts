/** Canvas colors come from CSS custom properties so themes stay in app.css. */
export interface Palette {
  wood: string;
  woodEdge: string;
  line: string;
  coordinate: string;
  blackStone: [string, string];
  whiteStone: [string, string];
  stoneShadow: string;
  lastMoveOnBlack: string;
  lastMoveOnWhite: string;
  tiers: string[];
  bestRing: string;
  candidateText: string;
  candidateOrder: string;
  ownershipBlack: string;
  ownershipWhite: string;
}

const FALLBACK: Palette = {
  wood: "#dcb46c",
  woodEdge: "#c99c50",
  line: "#3b2a14",
  coordinate: "#5a4321",
  blackStone: ["#4a4a4a", "#0b0b0b"],
  whiteStone: ["#ffffff", "#d4d0c8"],
  stoneShadow: "rgba(0, 0, 0, 0.35)",
  lastMoveOnBlack: "#f5f5f5",
  lastMoveOnWhite: "#1a1a1a",
  tiers: ["#16b8c6", "#3fae49", "#9ac230", "#e8b923", "#ec8a2b", "#d94b3d"],
  bestRing: "#0b6d76",
  candidateText: "#111111",
  candidateOrder: "#1a1206",
  ownershipBlack: "rgba(12, 12, 12, 1)",
  ownershipWhite: "rgba(250, 250, 250, 1)",
};

export function readPalette(element: Element | null): Palette {
  if (!element || typeof getComputedStyle !== "function") return FALLBACK;
  const style = getComputedStyle(element);
  const read = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  return {
    wood: read("--board-wood", FALLBACK.wood),
    woodEdge: read("--board-wood-edge", FALLBACK.woodEdge),
    line: read("--board-line", FALLBACK.line),
    coordinate: read("--board-coordinate", FALLBACK.coordinate),
    blackStone: [
      read("--stone-black-light", FALLBACK.blackStone[0]),
      read("--stone-black-dark", FALLBACK.blackStone[1]),
    ],
    whiteStone: [
      read("--stone-white-light", FALLBACK.whiteStone[0]),
      read("--stone-white-dark", FALLBACK.whiteStone[1]),
    ],
    stoneShadow: read("--stone-shadow", FALLBACK.stoneShadow),
    lastMoveOnBlack: read("--last-move-on-black", FALLBACK.lastMoveOnBlack),
    lastMoveOnWhite: read("--last-move-on-white", FALLBACK.lastMoveOnWhite),
    tiers: FALLBACK.tiers.map((color, index) => read(`--candidate-tier-${index}`, color)),
    bestRing: read("--candidate-best-ring", FALLBACK.bestRing),
    candidateText: read("--candidate-text", FALLBACK.candidateText),
    candidateOrder: read("--candidate-order", FALLBACK.candidateOrder),
    ownershipBlack: read("--ownership-black", FALLBACK.ownershipBlack),
    ownershipWhite: read("--ownership-white", FALLBACK.ownershipWhite),
  };
}
