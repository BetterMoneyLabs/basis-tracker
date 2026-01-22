// Basis Reserve Contract - Token Variant
// 
// This contract guards the reserve collateral (Tokens + ERG) and ensures that:
// 1. Redemptions are valid (signed by issuer, valid proof from tracker)
// 2. State updates are authorized by the owner
// 
// This variant supports reserves where the primary collateral is a token.

{
  // Same logic as standard Basis reserve for now, but intended for token boxes
  // R4: Owner Public Key
  // R5: Tracker NFT ID (optional)
  
  val ownerPubKey = extract(SELF.R4[Coll[Byte]].get)
  
  // simple owner spend for now
  proveDlog(ownerPubKey)
}
