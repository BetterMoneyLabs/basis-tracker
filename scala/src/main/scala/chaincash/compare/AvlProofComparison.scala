package chaincash.compare

import scorex.crypto.authds.avltree.batch._
import scorex.crypto.authds.{ADKey, ADValue}
import scorex.crypto.encode.Base16
import scorex.crypto.hash.{Blake2b256, Digest32}
import scorex.utils.Longs

/**
 * Standalone AVL proof comparison for the Basis reserve redemption.
 *
 * This object reproduces the AVL proofs using scorex's BatchAVLProver directly,
 * without the work.lithos.plasma dependency, so the results can be compared with
 * the Rust implementation (ergo-avltree-rust).
 *
 * Usage:
 *   sbt "runMain chaincash.compare.AvlProofComparison \
 *     --issuer-pubkey 037770... \
 *     --recipient-pubkey 03af... \
 *     --total-debt 50000000 \
 *     --timestamp 175... \
 *     --redeemed-amount 50000000"
 */
object AvlProofComparison extends App {

  private val KeyLength = 32

  case class CliArgs(
    issuerPubkey: String = "",
    recipientPubkey: String = "",
    totalDebt: Long = 0L,
    timestamp: Long = 0L,
    redeemedAmount: Long = 0L
  )

  def parseArgs(rawArgs: Array[String]): CliArgs = {
    rawArgs.sliding(2, 2).foldLeft(CliArgs()) { case (acc, Array(flag, value)) =>
      flag match {
        case "--issuer-pubkey" => acc.copy(issuerPubkey = value)
        case "--recipient-pubkey" => acc.copy(recipientPubkey = value)
        case "--total-debt" => acc.copy(totalDebt = value.toLong)
        case "--timestamp" => acc.copy(timestamp = value.toLong)
        case "--redeemed-amount" => acc.copy(redeemedAmount = value.toLong)
        case _ => acc
      }
    }
  }

  def hexToBytes(hex: String): Array[Byte] = Base16.decode(hex).get

  def keyHash(issuer: Array[Byte], recipient: Array[Byte]): Array[Byte] = {
    Blake2b256(issuer ++ recipient)
  }

  /**
   * Creates a new empty BatchAVLProver with the same parameters used by the Basis tracker.
   *
   * @param valueLengthOpt None for variable-length values (matching PlasmaParameters(32, None))
   */
  def emptyProver(valueLengthOpt: Option[Int] = None): BatchAVLProver[Digest32, Blake2b256.type] = {
    implicit val hf: Blake2b256.type = Blake2b256
    new BatchAVLProver[Digest32, Blake2b256.type](KeyLength, valueLengthOpt)
  }

  /**
   * Generate the reserve insert proof from an empty tree.
   *
   * Value format: timestamp (8 bytes BE) ++ redeemedAmount (8 bytes BE) = 16 bytes total.
   */
  def generateReserveInsertProof(
    issuer: Array[Byte],
    recipient: Array[Byte],
    timestamp: Long,
    redeemedAmount: Long
  ): (Array[Byte], Array[Byte], String) = {
    val prover = emptyProver(valueLengthOpt = None)
    val key = keyHash(issuer, recipient)
    val value = Longs.toByteArray(timestamp) ++ Longs.toByteArray(redeemedAmount)

    val insert = Insert(ADKey @@ key, ADValue @@ value)
    prover.performOneOperation(insert).get

    val proof = prover.generateProof()
    val digest = prover.digest

    (key, proof, Base16.encode(digest))
  }

  /**
   * Generate the tracker lookup proof.
   *
   * Tree stores hash(owner||receiver) -> totalDebt (8 bytes BE).
   */
  def generateTrackerLookupProof(
    issuer: Array[Byte],
    recipient: Array[Byte],
    totalDebt: Long
  ): (Array[Byte], Array[Byte], String) = {
    val prover = emptyProver(valueLengthOpt = None)
    val key = keyHash(issuer, recipient)
    val value = Longs.toByteArray(totalDebt)

    val insert = Insert(ADKey @@ key, ADValue @@ value)
    prover.performOneOperation(insert).get

    // Reset the prover's proof state so the lookup proof is standalone.
    // generateProof() commits the insert and clears the pending directions.
    prover.generateProof()

    val lookup = Lookup(ADKey @@ key)
    val foundValue = prover.performOneOperation(lookup).get
    val proof = prover.generateProof()
    val digest = prover.digest

    require(foundValue.exists(_.toSeq == value.toSeq), "Lookup did not return expected value")
    (key, proof, Base16.encode(digest))
  }

  def printSection(title: String): Unit = {
    println("=" * 60)
    println(title)
    println("=" * 60)
  }

  val cliArgs = parseArgs(this.args)

  require(cliArgs.issuerPubkey.nonEmpty, "--issuer-pubkey is required")
  require(cliArgs.recipientPubkey.nonEmpty, "--recipient-pubkey is required")
  require(cliArgs.totalDebt > 0, "--total-debt must be positive")
  require(cliArgs.timestamp > 0, "--timestamp must be positive")
  require(cliArgs.redeemedAmount > 0, "--redeemed-amount must be positive")

  val issuer = hexToBytes(cliArgs.issuerPubkey)
  val recipient = hexToBytes(cliArgs.recipientPubkey)
  require(issuer.length == 33, s"Issuer pubkey must be 33 bytes, got ${issuer.length}")
  require(recipient.length == 33, s"Recipient pubkey must be 33 bytes, got ${recipient.length}")

  printSection("Input Parameters")
  println(s"Issuer pubkey:      ${cliArgs.issuerPubkey}")
  println(s"Recipient pubkey:   ${cliArgs.recipientPubkey}")
  println(s"Total debt:         ${cliArgs.totalDebt}")
  println(s"Timestamp:          ${cliArgs.timestamp}")
  println(s"Redeemed amount:    ${cliArgs.redeemedAmount}")
  println()

  printSection("Computed Values")
  val key = keyHash(issuer, recipient)
  println(s"Note key (hash):    ${Base16.encode(key)}")
  println()

  printSection("Reserve AVL Proof (Insert)")
  val (reserveKey, reserveProof, reserveDigest) = generateReserveInsertProof(
    issuer, recipient, cliArgs.timestamp, cliArgs.redeemedAmount
  )
  println(s"Key:                ${Base16.encode(reserveKey)}")
  println(s"Insert proof bytes: ${Base16.encode(reserveProof)}")
  println(s"Insert proof size:  ${reserveProof.length}")
  println(s"Updated digest:     $reserveDigest")
  println()

  printSection("Tracker AVL Proof (Lookup)")
  val (trackerKey, trackerProof, trackerDigest) = generateTrackerLookupProof(
    issuer, recipient, cliArgs.totalDebt
  )
  println(s"Key:                ${Base16.encode(trackerKey)}")
  println(s"Lookup proof bytes: ${Base16.encode(trackerProof)}")
  println(s"Lookup proof size:  ${trackerProof.length}")
  println(s"Tree digest:        $trackerDigest")
  println()

  printSection("Serialized R5 (SAvlTree)")
  // Match Scala's ValueSerializer.serialize(AvlTreeConstant(tree)) format:
  // 0x64 || 33-byte digest || flags || VLQ keyLength || VLQ valueLength
  // flags = insert-only (0x01); keyLength = 32 (VLQ 0x20); valueLength = variable (VLQ 0x00)
  def vlqEncode(value: Int): Array[Byte] = {
    var v = value
    val bytes = scala.collection.mutable.ArrayBuffer[Byte]()
    while (v != 0) {
      var b = (v & 0x7f).toByte
      v >>= 7
      if (v != 0) b = (b | 0x80).toByte
      bytes += b
    }
    if (bytes.isEmpty) bytes += 0.toByte
    bytes.toArray
  }

  val r5Bytes = Array[Byte](0x64) ++
    hexToBytes(reserveDigest) ++
    Array[Byte](0x01) ++
    vlqEncode(32) ++
    vlqEncode(0)
  println(s"R5 hex:             ${Base16.encode(r5Bytes)}")
  println(s"R5 bytes:           ${r5Bytes.mkString(" ")}")
}
