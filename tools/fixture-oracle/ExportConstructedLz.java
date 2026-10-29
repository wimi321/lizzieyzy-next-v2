package featurecat.lizzie.rules;
import featurecat.lizzie.Lizzie;
import featurecat.lizzie.analysis.MoveData;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
public final class ExportConstructedLz {
  public static void main(String[] args) throws Exception {
    try (RulesLayerTestHarness env = RulesLayerTestHarness.open(9)) {
      String input = "(;FF[4]GM[1]SZ[9]KM[6.5]RU[Chinese]PB[Black]PW[White]DT[2000-01-01]C[Constructed fixture; analysis values are synthetic.];B[cc];W[gg])";
      BoardHistoryList history = SGFParser.parseSgf(input, true);
      Lizzie.board.setHistory(history);
      BoardData data = history.getStart().getVariation(0).orElseThrow().getData();
      data.engineName = "KataGo-fixture";
      data.setPlayouts(100);
      data.winrate = 58.5;
      data.isKataData = true;
      data.scoreMean = 2.5;
      data.scoreStdev = 3.0;
      data.pda = 0.0;
      data.rootVisits = 100;
      data.wrn = 0.04;
      data.wrnKnown = true;
      MoveData move = new MoveData();
      move.coordinate = "G3";
      move.playouts = 80;
      move.winrate = 58.5;
      move.policy = 30;
      move.scoreMean = 2.5;
      move.order = 0;
      move.variation = List.of("G3", "G7");
      data.bestMoves = List.of(move);
      String saved = SGFParser.saveToString(false);
      if (!saved.contains("LZ[")) throw new IllegalStateException("writer omitted LZ");
      Files.writeString(Path.of(args[0]), saved);
      System.out.println(saved);
    }
    System.exit(0);
  }
}
