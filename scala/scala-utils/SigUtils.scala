package chaincash.offchain

import sigmastate.basics.CryptoConstants
import special.sigma.GroupElement
import sigmastate.eval._
import sigmastate.basics.SecP256K1Group
import java.security.SecureRandom
import scala.annotation.tailrec
import scorex.crypto.hash.Blake2b256

object SigUtils {

  def randBigInt: BigInt = {
    val random = new SecureRandom()
    val values = new Array[Byte](32)
    random.nextBytes(values)
    BigInt(values).mod(SecP256K1Group.q)
  }

  /**
   * Signs `msg`, retrying with a fresh nonce until the signature is usable ON-CHAIN.
   *
   * Both the response `z` AND the Fiat-Shamir challenge `e` must be below 2^255. The reserve
   * contract verifies with signed big-endian reads (`byteArrayToBigInt`), so a 32-byte value
   * with the top bit set becomes NEGATIVE and `.exp(negative)` throws on-chain. Such a
   * signature is mathematically valid but rejected by the node, so both are regenerated here.
   */
  @tailrec
  def sign(msg: Array[Byte], secretKey: BigInt): (GroupElement, BigInt) = {
    val g: GroupElement = CryptoConstants.dlogGroup.generator

    val pk = g.exp(secretKey.bigInteger)

    val r = randBigInt
    val a: GroupElement = g.exp(r.bigInteger)
    val e = Blake2b256(a.getEncoded.toArray ++ msg ++ pk.getEncoded.toArray)
    val z = (r + secretKey * BigInt(e)) % CryptoConstants.groupOrder

    // A non-negative BigInt whose bitLength is 256 came from bytes with the top bit set --
    // exactly the case byteArrayToBigInt would read as negative. The challenge bytes must be
    // checked directly: BigInt(e) is negative for a top-bit-set hash, and a negative BigInt's
    // bitLength never exceeds 255, so a bitLength check on BigInt(e) would be a no-op.
    if (z.bitLength <= 255 && isContractCompatible(e)) {
      (a, z)
    } else {
      sign(msg,secretKey)
    }
  }

  /**
   * Whether a 32-byte big-endian value survives ErgoScript's signed `byteArrayToBigInt` read,
   * i.e. whether its top bit is clear. Mirrors `basis.offchain.SigUtils.isContractCompatible`.
   */
  def isContractCompatible(bytes32: Array[Byte]): Boolean =
    bytes32.nonEmpty && (bytes32(0) & 0x80.toByte) == 0

  /**
   * Verifies a Schnorr signature
   * @param msg Message that was signed
   * @param publicKey Public key
   * @param a Signature component (random point)
   * @param z Signature component (response)
   * @return true if signature is valid
   */
  def verify(msg: Array[Byte], publicKey: GroupElement, a: GroupElement, z: BigInt): Boolean = {
    val g: GroupElement = CryptoConstants.dlogGroup.generator
    val e = Blake2b256(a.getEncoded.toArray ++ msg ++ publicKey.getEncoded.toArray)
    val eBigInt = BigInt(e)
    val lhs = g.exp(z.bigInteger)
    val rhs = a.multiply(publicKey.exp(eBigInt.bigInteger))
    lhs == rhs
  }
}