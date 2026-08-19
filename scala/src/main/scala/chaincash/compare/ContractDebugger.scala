package chaincash.compare

import scorex.crypto.authds.avltree.batch._
import scorex.crypto.authds.{ADKey, ADValue, ADDigest, SerializedAdProof}
import scorex.crypto.encode.Base16
import scorex.crypto.hash.{Blake2b256, Digest32}
import scorex.utils.Longs
import sigma.crypto.CryptoConstants
import sigma.serialization.GroupElementSerializer
import sigma.GroupElement
import sigma.data.{CBigInt, CGroupElement}

import java.io.File
import scala.io.Source
import io.circe.parser._
import io.circe.Json

/**
 * Debugger for the Basis reserve redemption contract.
 *
 * Reads the unsigned transaction JSON (in /wallet/transaction/sign format) and checks
 * each contract condition independently. This helps identify which condition causes
 * "Script reduced to false" on the Ergo node.
 *
 * Usage:
 *   sbt "runMain chaincash.compare.ContractDebugger /tmp/redemption_tx_v7.json"
 */
object ContractDebugger extends App {

  private val KeyLength = 32

  // Simple resolver for BatchAVLVerifier (should not be called for self-contained proofs)
  private def simpleResolver(digest: Array[Byte]): scorex.crypto.authds.avltree.batch.ProverNodes[Digest32] = {
    throw new RuntimeException("Simple resolver should not be called")
  }

  // Helpers
  def hexToBytes(hex: String): Array[Byte] = Base16.decode(hex).get

  def keyHash(issuer: Array[Byte], recipient: Array[Byte]): Array[Byte] = Blake2b256(issuer ++ recipient)

  def bytesToLong(bytes: Array[Byte]): Long = {
    require(bytes.length == 8, s"Expected 8 bytes, got ${bytes.length}")
    Longs.fromByteArray(bytes)
  }

  def verifySchnorr(message: Array[Byte], pubkey: GroupElement, aBytes: Array[Byte], zBytes: Array[Byte]): Boolean = {
    val g = CGroupElement(CryptoConstants.dlogGroup.generator)
    val a = CGroupElement(GroupElementSerializer.fromBytes(aBytes).asInstanceOf[sigma.crypto.Platform.Ecp])
    val z = CBigInt(new java.math.BigInteger(1, zBytes))
    val e = Blake2b256(aBytes ++ message ++ pubkey.getEncoded.toArray)
    val eInt = CBigInt(new java.math.BigInteger(1, e))
    val lhs = g.exp(z)
    val rhs = a.multiply(pubkey.exp(eInt))
    lhs == rhs
  }

  def verifyAvlLookupProof(
    startingDigest: Array[Byte],
    proof: Array[Byte],
    key: Array[Byte],
    expectedValue: Array[Byte]
  ): Boolean = {
    implicit val hf: Blake2b256.type = Blake2b256
    val verifier = new BatchAVLVerifier[Digest32, Blake2b256.type](
      ADDigest @@ startingDigest,
      SerializedAdProof @@ proof,
      KeyLength,
      None,
      Some(1),
      Some(0)
    )
    val result = verifier.performOneOperation(Lookup(ADKey @@ key))
    result match {
      case scala.util.Success(Some(value)) => value.toArray sameElements expectedValue
      case _ => false
    }
  }

  def verifyAvlInsertProof(
    startingDigest: Array[Byte],
    proof: Array[Byte],
    key: Array[Byte],
    value: Array[Byte],
    expectedDigest: Array[Byte]
  ): Boolean = {
    implicit val hf: Blake2b256.type = Blake2b256
    val verifier = new BatchAVLVerifier[Digest32, Blake2b256.type](
      ADDigest @@ startingDigest,
      SerializedAdProof @@ proof,
      KeyLength,
      None,
      Some(1),
      Some(0)
    )
    val result = verifier.performOneOperation(Insert(ADKey @@ key, ADValue @@ value))
    result match {
      case scala.util.Success(_) =>
        verifier.digest.exists(_.toArray sameElements expectedDigest)
      case _ => false
    }
  }

  // Parse transaction JSON
  val txFile = this.args.headOption.getOrElse {
    println("Usage: ContractDebugger <transaction-json-file>")
    sys.exit(1)
  }

  val txJson = parse(Source.fromFile(txFile).mkString) match {
    case Right(json) => json
    case Left(err) =>
      println(s"Failed to parse transaction JSON: ${err.getMessage}")
      sys.exit(1)
  }

  val tx = txJson.hcursor.downField("tx").focus.getOrElse {
    println("Missing 'tx' field")
    sys.exit(1)
  }

  val inputs = tx.hcursor.downField("inputs").as[List[Json]].getOrElse(Nil)
  val dataInputs = tx.hcursor.downField("dataInputs").as[List[Json]].getOrElse(Nil)
  val outputs = tx.hcursor.downField("outputs").as[List[Json]].getOrElse(Nil)

  require(inputs.nonEmpty, "Transaction must have at least one input")
  require(outputs.nonEmpty, "Transaction must have at least one output")

  // Reserve input is the first input
  val reserveInput = inputs.head
  val reserveInputExtension = reserveInput.hcursor.downField("extension").as[Map[String, String]].getOrElse(Map.empty)
  val reserveInputBoxId = reserveInput.hcursor.downField("boxId").as[String].getOrElse("")

  // Reserve output is the first output
  val reserveOutput = outputs.head
  val reserveOutputR4 = reserveOutput.hcursor.downField("additionalRegisters").downField("R4").as[String].getOrElse("")
  val reserveOutputR5 = reserveOutput.hcursor.downField("additionalRegisters").downField("R5").as[String].getOrElse("")
  val reserveOutputR6 = reserveOutput.hcursor.downField("additionalRegisters").downField("R6").as[String].getOrElse("")
  val reserveOutputValue = reserveOutput.hcursor.downField("value").as[Long].getOrElse(0L)
  val reserveOutputErgoTree = reserveOutput.hcursor.downField("ergoTree").as[String].getOrElse("")
  val reserveOutputAssets = reserveOutput.hcursor.downField("assets").as[List[Map[String, Json]]].getOrElse(Nil)

  // Fetch reserve input box from Ergo node
  val nodeUrl = "http://127.0.0.1:9053"
  val apiKey = "hello"
  val reserveInputBoxJson = fetchBox(nodeUrl, apiKey, reserveInputBoxId)

  val reserveInputValue = reserveInputBoxJson.hcursor.downField("value").as[Long].getOrElse(0L)
  val reserveInputErgoTree = reserveInputBoxJson.hcursor.downField("ergoTree").as[String].getOrElse("")
  val reserveInputR4 = reserveInputBoxJson.hcursor.downField("additionalRegisters").downField("R4").as[String].getOrElse("")
  val reserveInputR5 = reserveInputBoxJson.hcursor.downField("additionalRegisters").downField("R5").as[String].getOrElse("")
  val reserveInputR6 = reserveInputBoxJson.hcursor.downField("additionalRegisters").downField("R6").as[String].getOrElse("")
  val reserveInputAssets = reserveInputBoxJson.hcursor.downField("assets").as[List[Map[String, Json]]].getOrElse(Nil)

  // Data input (tracker box) is the first data input
  val trackerBoxId = dataInputs.headOption.flatMap(_.hcursor.downField("boxId").as[String].toOption).getOrElse {
    println("Missing tracker data input")
    sys.exit(1)
  }
  val trackerBoxJson = fetchBox(nodeUrl, apiKey, trackerBoxId)
  val trackerR4 = trackerBoxJson.hcursor.downField("additionalRegisters").downField("R4").as[String].getOrElse("")
  val trackerR5 = trackerBoxJson.hcursor.downField("additionalRegisters").downField("R5").as[String].getOrElse("")
  val trackerAssets = trackerBoxJson.hcursor.downField("assets").as[List[Map[String, Json]]].getOrElse(Nil)
  val trackerNftId = trackerAssets.headOption.flatMap(_.get("tokenId").flatMap(_.asString)).getOrElse("")

  // Context variables
  val actionByte = decodeErgoByte(reserveInputExtension("0"))
  val receiverPubkeyHex = decodeErgoGroupElement(reserveInputExtension("1"))
  val reserveSig = decodeErgoCollBytes(reserveInputExtension("2"))
  val totalDebt = decodeErgoLong(reserveInputExtension("3"))
  val timestamp = decodeErgoLong(reserveInputExtension("4"))
  val reserveInsertProof = decodeErgoCollBytes(reserveInputExtension("5"))
  val trackerSig = decodeErgoCollBytes(reserveInputExtension("6"))
  val trackerLookupProof = decodeErgoCollBytes(reserveInputExtension("8"))

  // Derive owner pubkey from reserve input R4 (0x07 || 33 bytes)
  val ownerPubkey = CGroupElement(GroupElementSerializer.fromBytes(hexToBytes(reserveInputR4.drop(2))).asInstanceOf[sigma.crypto.Platform.Ecp])
  val receiverPubkey = CGroupElement(GroupElementSerializer.fromBytes(hexToBytes(receiverPubkeyHex)).asInstanceOf[sigma.crypto.Platform.Ecp])

  val key = keyHash(ownerPubkey.getEncoded.toArray, receiverPubkey.getEncoded.toArray)
  val message = key ++ Longs.toByteArray(totalDebt) ++ Longs.toByteArray(timestamp)

  // Print summary
  println("=" * 60)
  println("Basis Reserve Redemption Contract Debugger")
  println("=" * 60)
  println()
  println(s"Reserve input box:  $reserveInputBoxId")
  println(s"Reserve input value: $reserveInputValue")
  println(s"Reserve output value: $reserveOutputValue")
  println(s"Redeemed amount:    ${reserveInputValue - reserveOutputValue}")
  println(s"Total debt:         $totalDebt")
  println(s"Timestamp:          $timestamp")
  println()

  // Check each condition
  println("Condition checks:")
  println("-" * 60)

  // 1. selfPreserved
  val selfPreserved = {
    val propMatch = reserveOutputErgoTree == reserveInputErgoTree
    val tokensMatch = reserveOutputAssets.map(m => (m("tokenId").asString.get, m("amount").asNumber.get.toLong.get)) ==
      reserveInputAssets.map(m => (m("tokenId").asString.get, m("amount").asNumber.get.toLong.get))
    val r4Match = reserveOutputR4 == reserveInputR4
    val r6Match = reserveOutputR6 == reserveInputR6
    propMatch && tokensMatch && r4Match && r6Match
  }
  println(f"1. selfPreserved:           $selfPreserved%5s")
  println(s"   - propositionBytes match: ${reserveOutputErgoTree == reserveInputErgoTree}")
  println(s"   - tokens match:           ${reserveOutputAssets.map(m => (m("tokenId").asString.get, m("amount").asNumber.get.toLong.get)) == reserveInputAssets.map(m => (m("tokenId").asString.get, m("amount").asNumber.get.toLong.get))}")
  println(s"   - R4 match:               ${reserveOutputR4 == reserveInputR4}")
  println(s"   - R6 match:               ${reserveOutputR6 == reserveInputR6}")

  // 2. trackerIdCorrect
  val trackerIdCorrect = {
    val reserveR6Value = hexToBytes(reserveInputR6.drop(4)) // 0e20 prefix + 32 bytes
    trackerNftId == Base16.encode(reserveR6Value)
  }
  println(f"2. trackerIdCorrect:        $trackerIdCorrect%5s")
  println(s"   - tracker NFT:            $trackerNftId")
  println(s"   - reserve R6 value:       ${Base16.encode(hexToBytes(reserveInputR6.drop(4)))}")

  // 3. trackerDebtCorrect
  val trackerDigest = hexToBytes(trackerR5.drop(2).take(66)) // 0x64 + 33-byte digest
  val trackerValue = Longs.toByteArray(totalDebt)
  val trackerDebtCorrect = verifyAvlLookupProof(trackerDigest, trackerLookupProof, key, trackerValue)
  println(f"3. trackerDebtCorrect:       $trackerDebtCorrect%5s")

  // 4. timestampCorrect
  val storedTimestamp = 0L // first redemption, no lookup proof
  val timestampCorrect = timestamp > storedTimestamp
  println(f"4. timestampCorrect:         $timestampCorrect%5s")
  println(s"   - timestamp: $timestamp, storedTimestamp: $storedTimestamp")

  // 5. properRedemptionTree
  val reserveInputDigest = hexToBytes(reserveInputR5.drop(2).take(66))
  val reserveOutputDigest = hexToBytes(reserveOutputR5.drop(2).take(66))
  val reserveValue = Longs.toByteArray(timestamp) ++ Longs.toByteArray(reserveInputValue - reserveOutputValue)
  val properRedemptionTree = verifyAvlInsertProof(reserveInputDigest, reserveInsertProof, key, reserveValue, reserveOutputDigest)
  println(f"5. properRedemptionTree:     $properRedemptionTree%5s")

  // 6. properReserveSignature
  val reserveABytes = reserveSig.slice(0, 33)
  val reserveZBytes = reserveSig.slice(33, 65)
  val properReserveSignature = verifySchnorr(message, ownerPubkey, reserveABytes, reserveZBytes)
  println(f"6. properReserveSignature:   $properReserveSignature%5s")

  // 7. properlyRedeemed
  val redeemed = reserveInputValue - reserveOutputValue
  val alreadyRedeemed = 0L
  val debtDelta = totalDebt - alreadyRedeemed
  val trackerSigProvided = trackerSig.length > 0

  // Verify tracker signature
  val trackerABytes = trackerSig.slice(0, 33)
  val trackerZBytes = trackerSig.slice(33, 65)
  val trackerPubkey = CGroupElement(GroupElementSerializer.fromBytes(hexToBytes(trackerR4.drop(2))).asInstanceOf[sigma.crypto.Platform.Ecp])
  val properTrackerSignature = verifySchnorr(message, trackerPubkey, trackerABytes, trackerZBytes)
  val trackerSigValid = if (trackerSigProvided) properTrackerSignature else false
  val properlyRedeemed = (redeemed > 0) && (redeemed <= debtDelta) && trackerSigValid
  println(f"7. properlyRedeemed:         $properlyRedeemed%5s")
  println(s"   - redeemed: $redeemed, debtDelta: $debtDelta, trackerSigValid: $trackerSigValid")

  // 8. receiverCondition
  println(f"8. receiverCondition:       ${"n/a"}%5s  (requires proveDlog of receiver; cannot verify without private key)")
  println(s"   - receiver pubkey:        $receiverPubkeyHex")
  println()

  println("=" * 60)
  if (selfPreserved && trackerIdCorrect && trackerDebtCorrect && timestampCorrect &&
      properRedemptionTree && properReserveSignature && properlyRedeemed) {
    println("All verifiable conditions are TRUE.")
    println("The only remaining condition is receiverCondition (proveDlog(receiver)).")
    println("If the node rejects with 'Script reduced to false', the wallet likely cannot")
    println("satisfy proveDlog for the receiver public key.")
  } else {
    println("At least one verifiable condition is FALSE.")
  }
  println("=" * 60)

  // Helpers for fetching boxes and decoding Ergo constants
  def fetchBox(nodeUrl: String, apiKey: String, boxId: String): Json = {
    import java.net.{URL, HttpURLConnection}
    val url = new URL(s"$nodeUrl/blockchain/box/byId/$boxId")
    val conn = url.openConnection().asInstanceOf[HttpURLConnection]
    try {
      conn.setRequestMethod("GET")
      conn.setRequestProperty("api_key", apiKey)
      val response = Source.fromInputStream(conn.getInputStream).mkString
      parse(response) match {
        case Right(json) => json
        case Left(err) => throw new RuntimeException(s"Failed to parse box JSON: ${err.getMessage}")
      }
    } finally {
      conn.disconnect()
    }
  }

  def decodeErgoByte(hex: String): Byte = {
    require(hex.startsWith("02"), s"Expected Byte constant prefix 02, got $hex")
    hexToBytes(hex.drop(2)).head
  }

  def decodeErgoLong(hex: String): Long = {
    require(hex.startsWith("05"), s"Expected Long constant prefix 05, got $hex")
    val vlq = hexToBytes(hex.drop(2))
    var n = 0L
    var shift = 0
    var i = 0
    while (true) {
      val b = vlq(i)
      n |= (b & 0x7f).toLong << shift
      shift += 7
      i += 1
      if ((b & 0x80) == 0) {
        // zigzag decode
        return (n >>> 1) ^ (-(n & 1))
      }
    }
    throw new RuntimeException("Invalid VLQ encoding")
  }

  def decodeErgoCollBytes(hex: String): Array[Byte] = {
    require(hex.startsWith("0e"), s"Expected Coll[Byte] constant prefix 0e, got $hex")
    val bytes = hexToBytes(hex.drop(2))
    val len = bytes(0) & 0xff
    bytes.slice(1, 1 + len)
  }

  def decodeErgoGroupElement(hex: String): String = {
    require(hex.startsWith("07"), s"Expected GroupElement constant prefix 07, got $hex")
    hex.drop(2)
  }
}
