#!/usr/bin/env python3
"""Run the unmodified Java Tsumego algorithm with small I/O-only adapters.

Usage: python3 fixtures/tsumego/generate.py ../lizzieyzy-next
Requires javac/java. All classes are built in a temporary directory; the Java
reference repository is read-only. Adapters implement only board access,
addStone, warning collection and flatten capture, without GUI or engine code.
"""
from pathlib import Path
import hashlib
import random
import subprocess
import sys
import tempfile

REFERENCE = Path(sys.argv[1] if len(sys.argv) > 1 else "../lizzieyzy-next")
SOURCE = REFERENCE / "src/main/java/featurecat/lizzie/rules/Tsumego.java"
FILES = {
"featurecat/lizzie/Lizzie.java": r"""package featurecat.lizzie;
import featurecat.lizzie.rules.Board;
import java.util.ResourceBundle;
import java.util.ListResourceBundle;
public class Lizzie {
 public static Config config = new Config();
 public static Board board;
 public static class Config { public int tsumeGoWallDistance = 1; }
 public static ResourceBundle resourceBundle = new ListResourceBundle() {
  protected Object[][] getContents() { return new Object[][] {
   {"Tsumego.noRoomForKoThreat1", ""}, {"Tsumego.noRoomForKoThreat2", ""},
   {"Tsumego.noRoomForKoThreat.black", "B"}, {"Tsumego.noRoomForKoThreat.white", "W"}}; }
 };
}
""",
"featurecat/lizzie/rules/Stone.java": "package featurecat.lizzie.rules; public enum Stone { EMPTY, BLACK, WHITE }",
"featurecat/lizzie/rules/Zobrist.java": "package featurecat.lizzie.rules; public class Zobrist { public void toggleStone(int x,int y,Stone s) {} }",
"featurecat/lizzie/rules/extraMoveForTsumego.java": "package featurecat.lizzie.rules; public class extraMoveForTsumego { public int x,y; public Stone color; }",
"featurecat/lizzie/rules/Board.java": r"""package featurecat.lizzie.rules;
import java.util.*;
public class Board {
 public static int boardWidth,boardHeight;
 public Stone[] stones;
 public boolean black;
 public Double komi;
 public List<extraMoveForTsumego> added = new ArrayList<>();
 public Board(int w,int h,String input,boolean b) {
  boardWidth=w; boardHeight=h; black=b; stones=new Stone[w*h];
  for (int i=0;i<stones.length;i++) stones[i]=input.charAt(i)=='B'?Stone.BLACK:input.charAt(i)=='W'?Stone.WHITE:Stone.EMPTY;
 }
 public static int getIndex(int x,int y) { return y*boardWidth+x; }
 public Stone[] getStones(){return stones;}
 public Board getHistory(){return this;}
 public Zobrist getZobrist(){return new Zobrist();}
 public boolean isBlacksTurn(){return black;}
 public void flattenWithCondition(Stone[] s,Zobrist z,boolean b,List<extraMoveForTsumego> a,double k) { stones=s;black=b;added=a;komi=k; }
}
""",
"featurecat/lizzie/util/Utils.java": r"""package featurecat.lizzie.util;
import featurecat.lizzie.rules.*;
import java.util.*;
public class Utils {
 public static String warnings="";
 public static void showMsg(String s) { warnings += s.contains("[B]")?"B":"W"; }
 public static void addStone(Stone[] stones,Zobrist z,int x,int y,Stone color,List<extraMoveForTsumego> extraStones) {
  if(stones[Board.getIndex(x,y)]!=Stone.EMPTY)return;
  stones[Board.getIndex(x,y)]=color;z.toggleStone(x,y,color);
  extraMoveForTsumego stone=new extraMoveForTsumego();stone.x=x;stone.y=y;stone.color=color;extraStones.add(stone);
 }
}
""",
"Oracle.java": r"""import featurecat.lizzie.Lizzie;
import featurecat.lizzie.rules.*;
import featurecat.lizzie.util.Utils;
import java.io.*;
public class Oracle {
 public static void main(String[] args) throws Exception {
  BufferedReader in=new BufferedReader(new InputStreamReader(System.in));String line;
  while((line=in.readLine())!=null) {
   String[] a=line.split("\\|");int w=Integer.parseInt(a[1]),h=Integer.parseInt(a[2]);
   Lizzie.config.tsumeGoWallDistance=Integer.parseInt(a[3]);
   Board board=new Board(w,h,a[8],a[7].equals("B"));Lizzie.board=board;Utils.warnings="";
   Tsumego t=new Tsumego();Stone side=t.getCoverSideAndIndex(!a[4].equals("Auto"),a[4].equals("Black"));
   t.buildCoverWall(a[5].equals("Both")||a[5].equals("Defender"),a[5].equals("Both")||a[5].equals("Attacker"),!a[6].equals("Inherit"),a[6].equals("Black"));
   StringBuilder output=new StringBuilder(),added=new StringBuilder();
   for(Stone s:board.stones)output.append(s==Stone.BLACK?'B':s==Stone.WHITE?'W':'.');
   for(extraMoveForTsumego s:board.added)added.append(s.color==Stone.BLACK?'B':'W').append(s.x).append(',').append(s.y).append(';');
   System.out.println(line+"|"+output+"|"+(board.komi==null?"-":board.komi)+"|"+(side==Stone.BLACK?"B":"W")+"|"+(board.black?"B":"W")+"|"+Utils.warnings+"|"+added);
  }
 }
}
""",
}

def board(w, h, stones):
    data = ["."] * (w * h)
    for color, x, y in stones:
        data[y * w + x] = color
    return "".join(data)


def cases():
    center = [("W", 9, 9), ("W", 10, 9)] + [("B", x, y) for x, y in [(8,8),(9,8),(10,8),(11,8),(8,9),(11,9),(8,10),(9,10),(10,10),(11,10)]]
    corner = [("W",1,1),("W",2,1)] + [("B",x,y) for x,y in [(0,0),(0,1),(0,2),(1,2),(2,2),(3,1)]]
    edge = [("W",0,8),("W",1,8)] + [("B",x,y) for x,y in [(0,7),(1,7),(2,7),(2,8),(2,9),(0,9),(1,9)]]
    shapes = [("center",19,19,center),("center_white",19,19,[("B" if c=="W" else "W",x,y) for c,x,y in center]),("corner",19,19,corner)]
    for rotation in range(4):
        transformed = edge
        for _ in range(rotation): transformed = [(c,18-y,x) for c,x,y in transformed]
        shapes.append((f"edge_{rotation}",19,19,transformed))
    shapes += [("empty",19,19,[]),("single",19,19,[("B",0,0)]),("tie",19,19,[("B",8,8),("B",9,8),("W",8,9),("W",9,9)]),
               ("four_edges",19,19,[("B",0,9),("B",18,9),("B",9,0),("W",9,18)]),
               ("tall_edge",19,19,[(c,x,y) for y in range(1,18) for c,x in [("B",0),("W",1)]]),
               ("near_full",19,19,[("B" if (x+y)%2==0 else "W",x,y) for x in range(19) for y in range(19) if (x,y)!=(9,9)])]
    # Dense matrix covers Java's asymmetric half-board arithmetic, reflections,
    # area compensation, clipping and small/rectangular boards.
    for w,h in [(19,19),(13,13),(9,9),(9,13),(13,9),(8,12),(25,25),(2,2),(3,7)]:
        for x,y in [(0,0),(0,h//2),(w//2,0),(w-2,h//2),(w//2,h-2),(w//2,h//2)]:
            shapes.append((f"grid_{w}_{h}_{x}_{y}",w,h,[("B",x,y),("W",min(x+1,w-1),min(y+1,h-1))]))
    for name,w,h,stones in shapes:
        for distance in [1,3,8]:
            for ko in ["None","Attacker","Defender","Both"]:
                yield "|".join([name,str(w),str(h),str(distance),"Auto",ko,"Inherit","W",board(w,h,stones)])
    rng = random.Random(32)
    for i in range(64):
        w,h = rng.choice([(19,19),(13,19),(19,13),(12,16),(25,25)])
        occupied = rng.sample(range(w*h), rng.randrange(2,12))
        stones = [(rng.choice(["B","W"]), n%w,n//w) for n in occupied]
        for ko in ["None","Attacker","Defender","Both"]:
            yield "|".join([f"spread_{i}", str(w),str(h),str(1+i%3),"Auto",ko,"Inherit","B",board(w,h,stones)])
    for attack in ["Auto","Black","White"]:
        for first in ["Inherit","Black","White"]:
            for current in ["B","W"]:
                yield "|".join(["forced", "19", "19", "1",attack,"Both",first,current,board(19,19,center)])

with tempfile.TemporaryDirectory(prefix="tsumego-java-") as temporary:
    root = Path(temporary)
    files = dict(FILES)
    files["featurecat/lizzie/rules/Tsumego.java"] = SOURCE.read_text()
    for name, content in files.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
    subprocess.run(["javac", "-d", str(root), *map(str, root.rglob("*.java"))], check=True)
    output = subprocess.run(["java", "-cp", str(root), "Oracle"], input="\n".join(cases()) + "\n", text=True, capture_output=True, check=True).stdout
    destination = Path(__file__).with_name("java-golden.txt")
    destination.write_text("# Unmodified Java Tsumego.java SHA-256 " + hashlib.sha256(SOURCE.read_bytes()).hexdigest() + "\n" +
                           "# name|width|height|distance|attacker|ko|first|current|input|output|komi-or-noop|attacker-color|next|ko-warnings|ordered-added\n" + output)
    print(f"Generated {len(output.splitlines())} Java oracle cases in {destination}")
