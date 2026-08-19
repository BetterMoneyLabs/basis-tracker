package chaincash.compare

import com.google.common.primitives.Longs
import scorex.crypto.authds.{ADKey, ADValue}
import scorex.crypto.authds.avltree.batch.{BatchAVLProver, BatchAVLVerifier, Insert, InsertOrUpdate, Lookup}
import scorex.crypto.hash.{Blake2b256, Digest32}
import scorex.util.encode.Base16

/**
 * Cross-check: for an UPDATE of an existing key, does the JVM scrypto verifier's
 * final digest match the prover's digest? The Rust port (ergo_avltree_rust) shows
 * a divergence: prover digest != verifier digest for the update path.
 */
object AvlUpdateDigestCheck extends App {
  implicit val hf: Blake2b256.type = Blake2b256

  val key: ADKey = ADKey @@ Array.fill(32)(7.toByte)
  val v1: ADValue = ADValue @@ Array.fill(16)(1.toByte)
  val v2: ADValue = ADValue @@ Array.fill(16)(2.toByte)

  // Prover: insert v1, commit, then InsertOrUpdate v2, take proof + digest
  val prover = new BatchAVLProver[Digest32, Blake2b256.type](32, None)
  prover.performOneOperation(Insert(key, v1)).get
  prover.generateProof()
  val committedDigest = prover.digest

  prover.performOneOperation(InsertOrUpdate(key, v2)).get
  val proof = prover.generateProof()
  val proverDigest = prover.digest

  println(s"committed digest: ${Base16.encode(committedDigest)}")
  println(s"prover digest (after update): ${Base16.encode(proverDigest)}")

  // Verifier replays InsertOrUpdate from committed digest
  val verifier = new BatchAVLVerifier[Digest32, Blake2b256.type](
    committedDigest, proof, 32, None
  )
  val res = verifier.performOneOperation(InsertOrUpdate(key, v2))
  println(s"verifier op success: ${res.isSuccess}")
  val verifierDigest = verifier.digest.get
  println(s"verifier digest (InsertOrUpdate): ${Base16.encode(verifierDigest)}")

  // Verifier replays plain Insert (contract semantics) from committed digest
  val verifier2 = new BatchAVLVerifier[Digest32, Blake2b256.type](
    committedDigest, proof, 32, None
  )
  val res2 = verifier2.performOneOperation(Insert(key, v2))
  println(s"verifier plain Insert success: ${res2.isSuccess}")
  if (res2.isSuccess) {
    println(s"verifier digest (Insert): ${Base16.encode(verifier2.digest.get)}")
  }

  println(s"MATCH: ${Base16.encode(proverDigest) == Base16.encode(verifierDigest)}")
}
