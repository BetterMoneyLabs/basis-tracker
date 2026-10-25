package basis.contracts

import org.scalatest.{Matchers, PropSpec}
import scorex.util.encode.Base16
import sigma.ast.ErgoTree

import java.nio.charset.StandardCharsets
import java.nio.file.{Files, Paths}
import scala.util.matching.Regex

/**
 * Guards that the compiled contract artifacts hardcoded OUTSIDE the Scala tree stay in sync
 * with the actual contracts in `../contract/`:
 *
 *   - `crates/basis_store/src/contract_compiler.rs` — ErgoTree hex literals
 *   - `config/basis.toml.example` — P2S addresses
 *
 * The ErgoTree hex comparison normalizes the compiled tree through an address round trip
 * (fromProposition -> fromString): ergo-lib and the node normalize the constant table on
 * parse, so the canonical on-chain form of a script can differ from the raw compiler output
 * in constant indexing. The Rust literals must carry the normalized form, because that is
 * what ergo-lib produces when it parses the P2S and what lives on-chain in
 * `propositionBytes`. The P2S comparison needs no normalization (the address embeds the
 * compiler's serialization directly).
 */
class ContractArtifactSpec extends PropSpec with Matchers {

  private val projectRoot = Paths.get(sys.props("user.dir"))

  private val basisTreeHex = normalizedHex(BasisConstants.basisErgoTree)
  private val basisTokenTreeHex = normalizedHex(BasisConstants.basisTokenErgoTree)
  private val basisP2s = BasisConstants.basisAddress.toString
  private val basisTokenP2s = BasisConstants.basisTokenAddress.toString

  /** Parse the tree back out of its P2S address, reproducing the constant-table
   *  normalization ergo-lib and the node apply when a script is parsed. */
  private def normalizedHex(tree: ErgoTree): String = {
    val addr = BasisConstants.ergoAddressEncoder.fromProposition(tree).get
    val parsed = BasisConstants.ergoAddressEncoder.fromString(addr.toString).get
    Base16.encode(parsed.script.bytes)
  }

  private def readRepoFile(rel: String): String = {
    val p = projectRoot.resolve(rel)
    require(Files.exists(p), s"Missing file: $p (user.dir=${sys.props("user.dir")})")
    new String(Files.readAllBytes(p), StandardCharsets.UTF_8)
  }

  private def extract(r: Regex, text: String, what: String): String =
    r.findFirstMatchIn(text).map(_.group(1))
      .getOrElse(fail(s"Could not extract $what"))

  // First hex literal inside the given function body (the Ok("...") return value)
  private def rustFnHex(fnName: String): Regex = (fnName + """[^"]*"([0-9a-f]{100,})"""").r
  private def tomlValue(key: String): Regex = ("(?m)^" + key + """\s*=\s*"([^"]+)"""").r

  private lazy val rustSrc = readRepoFile("../crates/basis_store/src/contract_compiler.rs")
  private lazy val tomlExample = readRepoFile("../config/basis.toml.example")

  property("Rust contract_compiler.rs basis ErgoTree hex matches compiled ../contract/basis.es") {
    val rustHex = extract(rustFnHex("get_basis_reserve_ergo_tree_hex"), rustSrc, "basis ErgoTree hex")
    rustHex shouldEqual basisTreeHex
  }

  property("Rust contract_compiler.rs basis-token ErgoTree hex matches compiled ../contract/basis-token.es") {
    val rustHex = extract(rustFnHex("get_basis_token_reserve_ergo_tree_hex"), rustSrc, "basis-token ErgoTree hex")
    rustHex shouldEqual basisTokenTreeHex
  }

  property("config/basis.toml.example basis_reserve_contract_p2s matches compiled basis.es P2S") {
    val tomlP2s = extract(tomlValue("basis_reserve_contract_p2s"), tomlExample, "basis_reserve_contract_p2s")
    tomlP2s shouldEqual basisP2s
  }

  property("config/basis.toml.example basis_token_reserve_contract_p2s matches compiled basis-token.es P2S") {
    val tomlP2s = extract(tomlValue("basis_token_reserve_contract_p2s"), tomlExample, "basis_token_reserve_contract_p2s")
    tomlP2s shouldEqual basisTokenP2s
  }
}
