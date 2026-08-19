import sigmastate.serialization.ErgoTreeSerializer
import scorex.crypto.encode.Base16

object PrettyPrintTree extends App {
  val treeHex = scala.io.Source.stdin.getLines.mkString
  val bytes = Base16.decode(treeHex).get
  val tree = ErgoTreeSerializer.DefaultSerializer.deserializeErgoTree(bytes)
  println(tree)
  println("--- pretty ---")
  println(tree.toProposition(false))
}
