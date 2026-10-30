/** Canvas colors come from CSS custom properties so themes stay in app.css. */
export interface Palette {
  wood: string;
  woodEdge: string;
  woodGrain: string;
  woodVignette: string;
  boardShadow: string;
  line: string;
  coordinate: string;
  blackStone: [string, string, string];
  whiteStone: [string, string, string];
  whiteStoneEdge: string;
  stoneShadow: string;
  lastMoveOnBlack: string;
  lastMoveOnWhite: string;
  tiers: string[];
  bestRing: string;
  candidateText: string;
  candidateOrder: string;
  candidateShadow: string;
  ownershipBlack: string;
  ownershipWhite: string;
}

const FALLBACK: Palette = {
  wood: "#e2bd7a",
  woodEdge: "#cfa25a",
  woodGrain: "#7a4b1c",
  woodVignette: "rgba(96, 58, 18, 0.16)",
  boardShadow: "rgba(0, 0, 0, 0.55)",
  line: "#3b2a14",
  coordinate: "#5a4321",
  blackStone: ["#6b6b6b", "#262626", "#050505"],
  whiteStone: ["#ffffff", "#f1eee7", "#c9c2b4"],
  whiteStoneEdge: "rgba(70, 52, 24, 0.28)",
  stoneShadow: "rgba(0, 0, 0, 0.35)",
  lastMoveOnBlack: "#f5f5f5",
  lastMoveOnWhite: "#1a1a1a",
  tiers: ["#16b8c6", "#3fae49", "#9ac230", "#e8b923", "#ec8a2b", "#d94b3d"],
  bestRing: "#0b6d76",
  candidateText: "#111111",
  candidateOrder: "#1a1206",
  candidateShadow: "rgba(40, 22, 4, 0.35)",
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
    woodGrain: read("--board-wood-grain", FALLBACK.woodGrain),
    woodVignette: read("--board-wood-vignette", FALLBACK.woodVignette),
    boardShadow: read("--board-shadow", FALLBACK.boardShadow),
    line: read("--board-line", FALLBACK.line),
    coordinate: read("--board-coordinate", FALLBACK.coordinate),
    blackStone: [
      read("--stone-black-light", FALLBACK.blackStone[0]),
      read("--stone-black-mid", FALLBACK.blackStone[1]),
      read("--stone-black-dark", FALLBACK.blackStone[2]),
    ],
    whiteStone: [
      read("--stone-white-light", FALLBACK.whiteStone[0]),
      read("--stone-white-mid", FALLBACK.whiteStone[1]),
      read("--stone-white-dark", FALLBACK.whiteStone[2]),
    ],
    whiteStoneEdge: read("--stone-white-edge", FALLBACK.whiteStoneEdge),
    stoneShadow: read("--stone-shadow", FALLBACK.stoneShadow),
    lastMoveOnBlack: read("--last-move-on-black", FALLBACK.lastMoveOnBlack),
    lastMoveOnWhite: read("--last-move-on-white", FALLBACK.lastMoveOnWhite),
    tiers: FALLBACK.tiers.map((color, index) => read(`--candidate-tier-${index}`, color)),
    bestRing: read("--candidate-best-ring", FALLBACK.bestRing),
    candidateText: read("--candidate-text", FALLBACK.candidateText),
    candidateOrder: read("--candidate-order", FALLBACK.candidateOrder),
    candidateShadow: read("--candidate-shadow", FALLBACK.candidateShadow),
    ownershipBlack: read("--ownership-black", FALLBACK.ownershipBlack),
    ownershipWhite: read("--ownership-white", FALLBACK.ownershipWhite),
  };
}
