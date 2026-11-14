import type en from "../locales/en.json";

/** 人机对局 and the in-game controls: one section of every locale file. */
export type PlayLabels = (typeof en)["play"];
/** 引擎对局: the dialog, the score panel and the match controls. */
export type MatchLabels = (typeof en)["match"];
/** GTP 引擎 and the Gtp控制台. */
export type GtpLabels = (typeof en)["gtp"];
/** The rule presets' names, shared with 对局信息. */
export type RuleLabels = (typeof en)["info"]["rules"];
