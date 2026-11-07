// LizzieYzy Next's own toolbar icons (GPL-3.0, from its src/main/resources/assets).
// `?no-inline` keeps each one a file: Vite would inline files this small as data: URIs,
// which the packaged window's Content-Security-Policy refuses (img-src 'self').
import alternately from "../assets/java/hb.png?no-inline";
import allow from "../assets/java/blueallow.png?no-inline";
import avoid from "../assets/java/redavoid.png?no-inline";
import backMain from "../assets/java/backmain.png?no-inline";
import black from "../assets/java/smallblack1.png?no-inline";
import changeTurn from "../assets/java/pass.png?no-inline";
import clear from "../assets/java/x.png?no-inline";
import controlPanel from "../assets/java/control.png?no-inline";
import flash from "../assets/java/flash.png?no-inline";
import hawkeye from "../assets/java/hawkeye2.png?no-inline";
import markup from "../assets/java/mark1.png?no-inline";
import more from "../assets/java/horizonDown.png?no-inline";
import newFile from "../assets/java/newFile.png?no-inline";
import open from "../assets/java/open.png?no-inline";
import pass from "../assets/java/playpass.png?no-inline";
import rankMark from "../assets/java/RankMarkOn.png?no-inline";
import save from "../assets/java/save.png?no-inline";
import setMain from "../assets/java/setmain.png?no-inline";
import white from "../assets/java/smallwhite.png?no-inline";

export const JAVA_ICONS = {
  newFile,
  open,
  save,
  flash,
  hawkeye,
  rankMark,
  changeTurn,
  setMain,
  backMain,
  controlPanel,
  markup,
  black,
  white,
  alternately,
  pass,
  allow,
  avoid,
  more,
  clear,
} as const;
