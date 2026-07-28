Attempts to make P2P cash over the Internet started before Bitcoin, see, for example, "Peer-to-peer money: free currency over the internet" 
by Kenji Saito from 2003, or original RipplePay idea and service by Ryan Fugger from 2005. Cryptocurrency 
space ignored earlier work and started own attempts to do p2p cash, such as Lightning / Cashu / Fedimint etc.

Thus we have two non-intersecting worlds: original p2p cash which is based on p2p trust, and cryptocurrency-backed which 
does require for 100% backing with cryptocurrencies. We combine the best from two worlds in Basis: 
* money issuance can be based purely on trust
* optionally, on-chain reserves on Ergo can back issued p2p cash
* it is up to a peer to demand for backing, to choose whom to trust, whom to blacklist etc
* thus this is providing self-sovereign control on what kind of money (and so risk) to accept 
* we also call it free digital banking on steroids

Basis is a low-level framework which can be used in many monetary applications, such as:
* community currencies (LETS, local currencies etc)
* value transfer networks, informal (such as Hawala) and formal 
* agentic economies

and so on

There could be multiple coexisting Basis based communities (using different instances of the same software). They can always have economic connections via on-chain reserves, it would be good to 
explore more efficient options.

Whitepaper is at https://github.com/BetterMoneyLabs/chaincash/blob/master/docs/basis/basis.pdf 

Offchain server (under public domain license) https://github.com/BetterMoneyLabs/basis-tracker 

Working on a simple wallet now. Looking for communities willing to play with it!