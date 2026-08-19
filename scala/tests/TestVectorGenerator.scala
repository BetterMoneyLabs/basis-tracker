package chaincash.tests

import chaincash.offchain.SigUtils
import scorex.crypto.hash.Blake2b256
import scorex.crypto.encode.Base16
import sigmastate.basics.CryptoConstants
import special.sigma.GroupElement
import sigmastate.serialization.GroupElementSerializer

import java.security.SecureRandom

/**
 * Standalone test vector generator for Schnorr signature cross-validation.
 * Run once to generate deterministic test vectors, then delete this file.
 *
 * Usage: sbt "runMain chaincash.tests.TestVectorGenerator"
 */
object TestVectorGenerator {

  // Deterministic pseudo-random for reproducible test vectors
  private val seedRandom = new SecureRandom(Array[Byte](
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
    0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
    0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20
  ))

  def deterministicBigInt(): BigInt = {
    val values = new Array[Byte](32)
    seedRandom.nextBytes(values)
    BigInt(values).mod(CryptoConstants.dlogGroup.order)
  }

  case class TestVector(
    id: String,
    description: String,
    issuerPubkeyHex: String,
    recipientPubkeyHex: String,
    amount: Long,
    timestamp: Long,
    messageHex: String,
    signatureHex: String,
    shouldVerify: Boolean
  )

  def generateKeypair(): (BigInt, Array[Byte]) = {
    val secret = deterministicBigInt()
    val pk = CryptoConstants.dlogGroup.generator.exp(secret.bigInteger)
    val pkBytes = pk.getEncoded.toArray
    (secret, pkBytes)
  }

  def buildMessage(ownerPk: Array[Byte], receiverPk: Array[Byte], amount: Long, timestamp: Long): Array[Byte] = {
    val key = Blake2b256(ownerPk ++ receiverPk)
    val amountBytes = longToBytes(amount)
    val timestampBytes = longToBytes(timestamp)
    key ++ amountBytes ++ timestampBytes
  }

  def longToBytes(v: Long): Array[Byte] = {
    Array[Byte](
      ((v >> 56) & 0xFF).toByte,
      ((v >> 48) & 0xFF).toByte,
      ((v >> 40) & 0xFF).toByte,
      ((v >> 32) & 0xFF).toByte,
      ((v >> 24) & 0xFF).toByte,
      ((v >> 16) & 0xFF).toByte,
      ((v >>  8) & 0xFF).toByte,
      ( v        & 0xFF).toByte
    )
  }

  def signMessage(message: Array[Byte], secret: BigInt): Array[Byte] = {
    val (a, z) = SigUtils.sign(message, secret)
    GroupElementSerializer.toBytes(a) ++ z.toByteArray
  }

  def main(args: Array[String]): Unit = {
    println("=" * 80)
    println("Schnorr Signature Test Vector Generator")
    println("=" * 80)

    val vectors = scala.collection.mutable.ArrayBuffer[TestVector]()

    // Generate deterministic keypairs
    val (ownerSecret, ownerPk) = generateKeypair()
    val (receiverSecret, receiverPk) = generateKeypair()
    val (trackerSecret, trackerPk) = generateKeypair()
    val (wrongSecret, wrongPk) = generateKeypair()

    println(s"Owner pubkey:   ${Base16.encode(ownerPk)}")
    println(s"Receiver pubkey: ${Base16.encode(receiverPk)}")
    println(s"Tracker pubkey: ${Base16.encode(trackerPk)}")
    println(s"Wrong pubkey:   ${Base16.encode(wrongPk)}")
    println()

    // TV001: Standard valid signature
    {
      val amount = 1000000000L
      val timestamp = 1743379200000L
      val message = buildMessage(ownerPk, receiverPk, amount, timestamp)
      val signature = signMessage(message, ownerSecret)

      vectors += TestVector(
        "TV001",
        "Standard valid signature",
        Base16.encode(ownerPk),
        Base16.encode(receiverPk),
        amount,
        timestamp,
        Base16.encode(message),
        Base16.encode(signature),
        true
      )
    }

    // TV002: All-zero signature (invalid format)
    {
      val amount = 1000000000L
      val timestamp = 1743379200000L
      val message = buildMessage(ownerPk, receiverPk, amount, timestamp)
      val signature = Array.fill[Byte](65)(0)

      vectors += TestVector(
        "TV002",
        "All-zero signature should fail",
        Base16.encode(ownerPk),
        Base16.encode(receiverPk),
        amount,
        timestamp,
        Base16.encode(message),
        Base16.encode(signature),
        false
      )
    }

    // TV003: Valid tracker signature
    {
      val amount = 500000000L
      val timestamp = 1743379201000L
      val message = buildMessage(ownerPk, receiverPk, amount, timestamp)
      val signature = signMessage(message, trackerSecret)

      vectors += TestVector(
        "TV003",
        "Valid tracker signature",
        Base16.encode(trackerPk),
        Base16.encode(receiverPk),
        amount,
        timestamp,
        Base16.encode(message),
        Base16.encode(signature),
        true
      )
    }

    // TV004: Wrong signer signature
    {
      val amount = 1000000000L
      val timestamp = 1743379200000L
      val message = buildMessage(ownerPk, receiverPk, amount, timestamp)
      val signature = signMessage(message, wrongSecret)

      vectors += TestVector(
        "TV004",
        "Wrong signer signature should fail",
        Base16.encode(ownerPk),
        Base16.encode(receiverPk),
        amount,
        timestamp,
        Base16.encode(message),
        Base16.encode(signature),
        false
      )
    }

    // TV005: Corrupted signature (flip bit in a component)
    {
      val amount = 1000000000L
      val timestamp = 1743379200000L
      val message = buildMessage(ownerPk, receiverPk, amount, timestamp)
      val signature = signMessage(message, ownerSecret)
      signature(0) = (signature(0) ^ 0x01).toByte

      vectors += TestVector(
        "TV005",
        "Corrupted signature should fail",
        Base16.encode(ownerPk),
        Base16.encode(receiverPk),
        amount,
        timestamp,
        Base16.encode(message),
        Base16.encode(signature),
        false
      )
    }

    // TV006: Wrong amount
    {
      val amount = 1000000000L
      val timestamp = 1743379200000L
      val message = buildMessage(ownerPk, receiverPk, amount, timestamp)
      val signature = signMessage(message, ownerSecret)
      val wrongMessage = buildMessage(ownerPk, receiverPk, amount + 1, timestamp)

      vectors += TestVector(
        "TV006",
        "Wrong amount should fail",
        Base16.encode(ownerPk),
        Base16.encode(receiverPk),
        amount,
        timestamp,
        Base16.encode(wrongMessage),
        Base16.encode(signature),
        false
      )
    }

    // TV007: Wrong timestamp
    {
      val amount = 1000000000L
      val timestamp = 1743379200000L
      val message = buildMessage(ownerPk, receiverPk, amount, timestamp)
      val signature = signMessage(message, ownerSecret)
      val wrongMessage = buildMessage(ownerPk, receiverPk, amount, timestamp + 1)

      vectors += TestVector(
        "TV007",
        "Wrong timestamp should fail",
        Base16.encode(ownerPk),
        Base16.encode(receiverPk),
        amount,
        timestamp,
        Base16.encode(wrongMessage),
        Base16.encode(signature),
        false
      )
    }

    // TV008: Wrong recipient
    {
      val amount = 1000000000L
      val timestamp = 1743379200000L
      val message = buildMessage(ownerPk, receiverPk, amount, timestamp)
      val signature = signMessage(message, ownerSecret)
      val wrongMessage = buildMessage(ownerPk, wrongPk, amount, timestamp)

      vectors += TestVector(
        "TV008",
        "Wrong recipient should fail",
        Base16.encode(ownerPk),
        Base16.encode(receiverPk),
        amount,
        timestamp,
        Base16.encode(wrongMessage),
        Base16.encode(signature),
        false
      )
    }

    // TV009: Max u64 values
    {
      val amount = -1L // u64::MAX
      val timestamp = -1L // u64::MAX
      val message = buildMessage(ownerPk, receiverPk, amount, timestamp)
      val signature = signMessage(message, ownerSecret)

      vectors += TestVector(
        "TV009",
        "Maximum u64 values",
        Base16.encode(ownerPk),
        Base16.encode(receiverPk),
        amount,
        timestamp,
        Base16.encode(message),
        Base16.encode(signature),
        true
      )
    }

    // TV010: Zero values
    {
      val amount = 0L
      val timestamp = 0L
      val message = buildMessage(ownerPk, receiverPk, amount, timestamp)
      val signature = signMessage(message, ownerSecret)

      vectors += TestVector(
        "TV010",
        "Zero amount and timestamp",
        Base16.encode(ownerPk),
        Base16.encode(receiverPk),
        amount,
        timestamp,
        Base16.encode(message),
        Base16.encode(signature),
        true
      )
    }

    // TV011: Emergency redemption same message
    {
      val amount = 500000000L
      val timestamp = 1743379202000L
      val message = buildMessage(ownerPk, receiverPk, amount, timestamp)
      val reserveSig = signMessage(message, ownerSecret)

      vectors += TestVector(
        "TV011",
        "Emergency redemption valid reserve signature",
        Base16.encode(ownerPk),
        Base16.encode(receiverPk),
        amount,
        timestamp,
        Base16.encode(message),
        Base16.encode(reserveSig),
        true
      )
    }

    // Print as JSON
    println("\n" + "=" * 80)
    println("TEST VECTORS (JSON)")
    println("=" * 80)
    println("[")
    vectors.zipWithIndex.foreach { case (v, i) =>
      val comma = if (i < vectors.length - 1) "," else ""
      println(s"  {")
      println(s"    \"id\": \"${v.id}\",")
      println(s"    \"description\": \"${v.description}\",")
      println(s"    \"issuer_pubkey\": \"${v.issuerPubkeyHex}\",")
      println(s"    \"recipient_pubkey\": \"${v.recipientPubkeyHex}\",")
      println(s"    \"amount\": ${v.amount},")
      println(s"    \"timestamp\": ${v.timestamp},")
      println(s"    \"message\": \"${v.messageHex}\",")
      println(s"    \"signature\": \"${v.signatureHex}\",")
      println(s"    \"should_verify\": ${v.shouldVerify}")
      println(s"  }$comma")
    }
    println("]")

    // Print as Rust code
    println("\n" + "=" * 80)
    println("TEST VECTORS (Rust code)")
    println("=" * 80)
    println("pub const SCHNORR_TEST_VECTORS: &[SchnorrTestVector] = &[")
    vectors.foreach { v =>
      println(s"    SchnorrTestVector {")
      println(s"        id: \"${v.id}\",")
      println(s"        description: \"${v.description}\",")
      println(s"        issuer_pubkey_hex: \"${v.issuerPubkeyHex}\",")
      println(s"        recipient_pubkey_hex: \"${v.recipientPubkeyHex}\",")
      println(s"        amount: ${v.amount}u64,")
      println(s"        timestamp: ${v.timestamp}u64,")
      println(s"        message_hex: \"${v.messageHex}\",")
      println(s"        signature_hex: \"${v.signatureHex}\",")
      println(s"        should_verify: ${v.shouldVerify},")
      println(s"    },")
    }
    println("];")

    // Print as Markdown for specs
    println("\n" + "=" * 80)
    println("TEST VECTORS (Markdown for specs)")
    println("=" * 80)
    vectors.foreach { v =>
      println(s"\n#### ${v.id} - ${v.description}")
      println(s"- **Issuer pubkey**: `${v.issuerPubkeyHex}`")
      println(s"- **Recipient pubkey**: `${v.recipientPubkeyHex}`")
      println(s"- **Amount**: `${v.amount}`")
      println(s"- **Timestamp**: `${v.timestamp}`")
      println(s"- **Message**: `${v.messageHex}`")
      println(s"- **Signature**: `${v.signatureHex}`")
      println(s"- **Expected verify**: `${v.shouldVerify}`")
    }

    println("\n" + "=" * 80)
    println(s"Generated ${vectors.length} test vectors")
    println("=" * 80)
  }
}
