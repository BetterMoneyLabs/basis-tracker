package basis.offchain

import sigma.crypto.CryptoConstants
import sigma.GroupElement
import sigma.crypto.SecP256K1Group
import sigma.data.{CBigInt, CGroupElement}
import java.security.SecureRandom
import scala.annotation.tailrec
import scorex.crypto.hash.Blake2b256

object SigUtils {

  implicit def javaBigIntegerToSigmaBigInt(b: java.math.BigInteger): sigma.BigInt = CBigInt(b)

  implicit def scalaBigIntToSigmaBigInt(b: BigInt): sigma.BigInt = CBigInt(b.bigInteger)

  implicit def groupElementToEcPointType(ge: GroupElement): sigma.crypto.EcPointType =
    ge.asInstanceOf[CGroupElement].wrappedValue

  implicit def ecPointTypeToGroupElement(ep: sigma.crypto.EcPointType): GroupElement =
    CGroupElement(ep)

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
   * contract verifies with
   *
   * {{{
   *   val zInt = byteArrayToBigInt(zBytes)          // SIGNED big-endian
   *   val eInt = byteArrayToBigInt(blake2b256(a ++ message ++ pubkey))
   *   g.exp(zInt) == a.multiply(pubkey.exp(eInt))
   * }}}
   *
   * `byteArrayToBigInt` is a two's-complement conversion, so a 32-byte value with the top bit set
   * becomes NEGATIVE, and `.exp(negative)` throws. Such a signature is mathematically valid but
   * rejected by the node.
   *
   * The `z.bitLength <= 255` check was already here; the challenge check was missing, which meant
   * roughly half of all signatures produced by this reference implementation (and therefore by
   * TV003 in `specs/SCHNORR_SIGNATURE_SPEC.md`) could not be redeemed on-chain. The Rust signer
   * enforced both rules already.
   */
  @tailrec
  def sign(msg: Array[Byte], secretKey: BigInt): (GroupElement, BigInt) = {
    val g: GroupElement = CGroupElement(CryptoConstants.dlogGroup.generator)

    val pk = g.exp(secretKey.bigInteger)

    val r = randBigInt
    val a: GroupElement = g.exp(r.bigInteger)
    val e = Blake2b256(a.getEncoded.toArray ++ msg ++ pk.getEncoded.toArray)
    val z = (r + secretKey * BigInt(e)) % SecP256K1Group.q

    // Scala's BigInt applies two's-complement for negative values, so a non-negative BigInt whose
    // bitLength is 256 came from bytes with the top bit set -- exactly the case byteArrayToBigInt
    // would read as negative.
    val zContractSafe = z.bitLength <= 255
    val eContractSafe = BigInt(e).bitLength <= 255

    if (zContractSafe && eContractSafe) {
      (a, z)
    } else {
      sign(msg, secretKey)
    }
  }

  /**
   * Whether a 32-byte big-endian value survives ErgoScript's signed `byteArrayToBigInt` read, i.e.
   * whether its top bit is clear. Mirrors `basis_core::impls::is_contract_compatible_be32`.
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
    val g: GroupElement = CGroupElement(CryptoConstants.dlogGroup.generator)
    val e = Blake2b256(a.getEncoded.toArray ++ msg ++ publicKey.getEncoded.toArray)
    val eBigInt = BigInt(e)
    val lhs = g.exp(z.bigInteger)
    val rhs = a.multiply(publicKey.exp(eBigInt.bigInteger))
    lhs == rhs
  }
}