use bytes::Bytes;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use ergo_avltree_rust::authenticated_tree_ops::*;
use ergo_avltree_rust::batch_avl_verifier::BatchAVLVerifier;
use ergo_avltree_rust::batch_node::*;
use ergo_avltree_rust::operation::*;
use ergo_avltree_rust::persistent_batch_avl_prover::*;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Instant;

mod common;
use common::*;

#[test]
fn test_removed_nodes() {
    // return removed leafs and internal nodes for small tree
    /*
     * manual check, that correct leafs and internal nodes where deleted
     * ______________top(V9WUMj6P,ES5Rnuf1)                                         top2(5VjCEAdt,2VT2d2nG)
     * __________________/           \                                                       /   \
     * NInf(11111111,ChyvjCc9)    right(5VjCEAdt,26ouau2w)       =>   NInf(11111111,DuQAiTxk)     Leaf1(5VjCEAdt,A889CP2P)
     * __________________________________/     \
     * __________Leaf0(V9WUMj6P,Fx5gbhBF)      Leaf1(5VjCEAdt,A889CP2P)
     */
    let mut prover = generate_and_populate_prover(2).0;
    let top = prover.top_node(); // V9WUMj6P,ES5Rnuf1
    let negative_infinity = top.borrow().left(); // 11111111,ChyvjCc9
    let right = top.borrow().right(); // 5VjCEAdt,26ouau2w
    let leaf0 = right.borrow().left(); // V9WUMj6P,Fx5gbhBF
    let leaf1 = right.borrow().right(); // 5VjCEAdt,A889CP2P

    let all = [
        leaf1.clone(),
        top.clone(),
        right.clone(),
        leaf0.clone(),
        negative_infinity.clone(),
    ];
    for n in &all {
        assert!(prover.contains(n));
    }
    let removed_manual = &all[1..];

    let key = leaf0.borrow().key();
    prover
        .perform_one_operation(&Operation::Remove(key))
        .unwrap();
    let key = leaf1.borrow().key();
    prover
        .perform_one_operation(&Operation::Lookup(key))
        .unwrap();

    let removed = prover.removed_nodes();

    // Top, Right and Leaf0 are not on the path any more, NegativeInfinity.newNextLeafKey changed.
    // Leaf1 is not affected
    assert_eq!(removed.len(), removed_manual.len());
    for n in removed_manual {
        assert!(removed
            .iter()
            .find(|rn| {
                let l1 = rn.borrow_mut().label();
                let l2 = n.borrow_mut().label();
                l1 == l2
            })
            .is_some());
    }
    prover.generate_proof();
}

fn ins_op(key: &[u8], value: &[u8]) -> Operation {
    Operation::Insert(KeyValue {
        key: Bytes::copy_from_slice(&base16::decode(key).unwrap()),
        value: Bytes::copy_from_slice(&base16::decode(value).unwrap()),
    })
}

fn rem_op(key: &[u8]) -> Operation {
    Operation::Remove(Bytes::copy_from_slice(&base16::decode(key).unwrap()))
}

#[test]
fn test_removed_nodes_special_cases() {
    // removedNodes() special case
    let mut prover = generate_and_populate_prover(0).0;
    let in_list = [
        ins_op(
            b"333724f1e5ed593ff3760e2fd14257e53320fcaba195198fe364c18317c8357a",
            b"e6e95bbc282023dc0f319a10a4166099",
        ),
        ins_op(
            b"70dc695bd7fb4f8d40f69cd82e6d704f219b7d49efee7856a54b809019bcb281",
            b"aab1db7e6768e75c360d6145563913a4",
        ),
        ins_op(
            b"ccdee12ce7d48bce1f2f3b237dafb03fe89109da05ad3e86ca1add4a969c6f11",
            b"3e1f7320a5560a034e37a51d7f2a5187",
        ),
        ins_op(
            b"71eb525ad833b9ac5e35feed6a4e663125cf764069a4c4bc69e135b19b84fc20",
            b"cc688ec7736770ba2cde8e8e43ef4730",
        ),
        ins_op(
            b"0f09f7704ed285c56154a2f7aef6eadde3e0faf9f3f07bb84393d7f83f8f9669",
            b"62116adc67d12aea779cf2eadb8854ea",
        ),
        ins_op(
            b"e73879715ba969d9f2f2ec1970ce379fec7e770e570e9fd454714ed874039236",
            b"e1fdbb6a2e7045c44d251b35822b8f9c",
        ),
        ins_op(
            b"66566ba505b632ce2fe70973bd2f695f62ca61edf78dd855424e1b80ed9db7ef",
            b"2bb222b9e052ce69246c4461975e7fef",
        ),
        ins_op(
            b"c2b09394bfffe27d5c1b8ba6f67bd489b499864531c85e3c3684821f6e38729d",
            b"9630dba1be5b809073bab215ad32d0ce",
        ),
        ins_op(
            b"fcca30ec0255f4117b71bb705f7f2e59e7222774da1dfae72c5ba032330c457a",
            b"f6d131b192aa421d444fd4e5b6088428",
        ),
        ins_op(
            b"f8d58130ccca6981df20ec8117039e390031dbe7caee3453d18b08d8df860709",
            b"6912d475249221a947c31a7c435a111f",
        ),
        ins_op(
            b"e0723e441695e2886e2450999a996788595dea7d25e508209a14472e06f68b36",
            b"42e6f2e681d979097205822d73b9fc4c",
        ),
        ins_op(
            b"628181857476ed88dcbf0d77c460b8626cd968a9bdf43c1d2ea67f2acd3694d3",
            b"940746265213123f4149a49ebad3030a",
        ),
        ins_op(
            b"4a83fefebe881f26974969c3127ebbfe711358cb89465bc3b186f895bcd61f5e",
            b"e77b0de31e457f58318a65fc169060e5",
        ),
        ins_op(
            b"0d58e0cda79b82eddf8d518d4a1addf5a92447091d78bcecdb727ab0c81d7a7b",
            b"9b01651c3362cc1964f81f3af4c73aea",
        ),
        ins_op(
            b"bb13eaafd61c429894899d4ed304dacc5ed6d7e962c5df7c94b0a67e73f84783",
            b"5b5bb917a0f051395c425920f084fce2",
        ),
        ins_op(
            b"a4ca2cad4e5662c3bfdd38df5fc6dd158e125cbde07a4c549431b38da3a61c44",
            b"4440d1c4032329a4408f0d84baf4ae24",
        ),
        ins_op(
            b"fe95b3aad90f7be6345c7e3a321e3e4b211929bdaa0dfe3d5d37410f2ff2bfbf",
            b"5f7b9c94e7d96216779b2c360b9ef040",
        ),
        ins_op(
            b"41c55b11a7f61fd17ad228a98195383b5fc084bcc1689839bf4a8f858dbe5390",
            b"b356acb04eefe3687b2cd211a5989b21",
        ),
        ins_op(
            b"92982f135c505f4411116f8e6a3d6f4b05468cc175df63713adca98fbb2ca21e",
            b"4283e20470f9db447aa7e2ff4c75dd1f",
        ),
        ins_op(
            b"9f26be0d45df9f7d8193322a780823b6a42e39415f62ea8a431a3579c2d89890",
            b"59405deb9da456de5a2f6048052dcfe7",
        ),
        ins_op(
            b"08df47d9e21a228b56872d187e7ca0033dfe93fc3afa1d1f754d6a40a427524e",
            b"44063e6f03f96415569d6262775712f9",
        ),
        ins_op(
            b"58b5f141d5b0d4f4c991e8ebdc6e73b3fb661d6c4e29eed688b70850d37cff62",
            b"659387d105856c8859ec1d78e34a65e5",
        ),
        ins_op(
            b"bcaa68a3b5f6fc7ae09a5f4f3156db9d556a6e858811a2973f77eee881167b69",
            b"bb23952d560dace56e9bf3170b1f9090",
        ),
        ins_op(
            b"053d23c7067a430081b807b95b96ec914ecb974275785953236eddec3c78aa9c",
            b"678370b4aa1c70170c1228e344e0fe02",
        ),
        ins_op(
            b"d37aeeca4ad298dccf1885946c7ea3cc573342a357b7387c23cf4d7fa7103f3b",
            b"b79baf3104d094307c98ee33bfb23049",
        ),
        ins_op(
            b"d8795a288935436eee56b8af32504a76ac33895a827f190d69c0329285c1a4ef",
            b"9db3e32a35f3f082734da766fd76afe1",
        ),
        ins_op(
            b"fa99ea1df1e1ec49ede3ecf66c5a9473c0bd0603eceae251771c22bd2ba84566",
            b"3c77ace7964560d62915639d25b010a8",
        ),
        ins_op(
            b"aaa10a1f6442ab992e7b17818c72c50f3486b22a02dfc7edeb24ec93d13fc7c1",
            b"98f17e60d8c8cd7237906873e569d113",
        ),
        ins_op(
            b"e4d5a2975d563d0ee36b581aa024549fe3cdc7a5f682d037ff64b93f835aa495",
            b"7f5eb623237b593e291d6e74be684077",
        ),
        ins_op(
            b"3eef10eaab9e14d6329a74dde04575747e3b1b10e8f8413d1f474392d3952f88",
            b"863522e89cb49b3a3f3b60c517aa742e",
        ),
        ins_op(
            b"2dca8ddb1ffbb4e6d25aa9ac83edd73c7c47da9c2c671a3b4f79eb69bf0314ea",
            b"b8061cec7190062643ca3ec7c3bd03d9",
        ),
        ins_op(
            b"8c1fb9dbe4310f55128fb4bc8428957628757a42dde016a0bf15f59352eac8ed",
            b"9bd7b75c06b002762e10b9cb5869c005",
        ),
        ins_op(
            b"63f5e594f305b10bd6d4efa17c7fc64855aa75f4241451d2440e1890c21de5d1",
            b"be156e894cb55e0e36091ea96811841d",
        ),
        ins_op(
            b"dc021821a7b8c6c4149b1d6340c373c3d072b9eea3eb33b2a9a30ad8f0444c15",
            b"2716a7a3b6a28eab0718e8ee17dbec47",
        ),
        ins_op(
            b"a0e3dbe575c3d386c87edf9b038ad1dc85c16673a9f9822bda12c028d3c78d17",
            b"3b9381720dfe2b867428be45f0e8ab0f",
        ),
        ins_op(
            b"3fbbac896a6f6189f29f7440be817332de6f8d2d72f2d55d8c5f2d7a08d9ba4c",
            b"a859a79147a4fa1668034d261756e48a",
        ),
        ins_op(
            b"cd6050353dbfe56285929b17ef824565e1e6ee9776ef2e30719296e5638f1f83",
            b"096b363c35f8ae5376cae0e7a598b916",
        ),
        ins_op(
            b"3ee0e1bebd6f4a3b48e45a3036aa13bdf91915066af52ac082c128220072e559",
            b"d3f86ffb7237259e2c366789879c5dc2",
        ),
        ins_op(
            b"6a7fc569c17acdd915d057c5a3a474c2a46084dc5f44604ad4c0a6c10763eecd",
            b"8309468daa78abaa3342ecccef317808",
        ),
        ins_op(
            b"c64af22acee45a41ec9dad0c22663588437878c88bec56a31a3740f13e57a54c",
            b"92243f940c1d503e4cd55170117540f7",
        ),
        ins_op(
            b"62c53a6093e9f3dec35db79351227c1fddceea71954a1496ba1b8299f2d27b8b",
            b"cd0121eba771785f47417c877696dfe2",
        ),
        ins_op(
            b"15206aba0f8bf92e8ea9d4d9eb2db214c6086da88605180f090024183ca4ec21",
            b"b61777769af56cd0742fc2160c9336dd",
        ),
        ins_op(
            b"a8296d3733c1ac6981e1027908f5a4ee3839d0fa76ffc84a5e117fd4b3c1288c",
            b"11c6cc6080a083393873da0e37f41a5b",
        ),
        ins_op(
            b"fd73b884610f1ec81d000a957ba21facb57a8dcb136b8b9228072de31772d3d5",
            b"666d33ce07008ce10123f7ccef274629",
        ),
        ins_op(
            b"1ea6aac667fc1a99abbb04b45128d8b07b42928e399ec8a4822ab5978373fa14",
            b"9f4719db94e10cdf13b77f5ef3fe7d3a",
        ),
        ins_op(
            b"0362af1eaf3b25f71fb1db4775ae57264e5fc9f933faa3f5bb2012c95d8754cc",
            b"95b4775334f2d4120b6ea12e9d75c632",
        ),
        ins_op(
            b"f1cb97a2f42f497e2292f1faeee22996c4e5da3c02beb757417125338ed2d28b",
            b"e62b628a0d52477a16fb486f7a5ee886",
        ),
        ins_op(
            b"fe4938a1c6a505c8180f15907672eebc736014e795d6f92ae465848d92210615",
            b"55af2c2e1cf22737595f230b0edeee42",
        ),
        ins_op(
            b"6e9c564e1190f821aae7326dee8a31b16dd53216dd724011f21c09c0ac7909bc",
            b"84d7eea349dc6c676f5c05c70a6c66a0",
        ),
        ins_op(
            b"f139bc505fa76567197a197307cf668b3c8a858b40d3fd54d66099b9baa0fde4",
            b"25ac61e586c276561b28a34e9421e32a",
        ),
        ins_op(
            b"78ac97b232880841dc4d417274486f5db9e3cb5a84d44a94624ecf2e77d1eeea",
            b"caf7061b8cc30f08792ebbf4ee2333cf",
        ),
        ins_op(
            b"369b593ae971613636048143b39d895148aca2eed84f91bba2a03009f2e30c68",
            b"97d584cff7c21d040b1a10783998ff7b",
        ),
        ins_op(
            b"419b1c45fcb6080fe8d091474e71b413974e2848dd1c823c8e809fb1d71f0f23",
            b"d77a5e35a4455a3e2036fa9d4a45cf11",
        ),
        ins_op(
            b"75c98f00c72fd894663d0cbd7f25d6aa890d76e1091eebb0a2d888e7be20e054",
            b"8c23c50f92960f1a125313050812858f",
        ),
        ins_op(
            b"c1b74d2971a2a0d09896d7f903692c71c8bc6a1b745d3fc4e70065d4beed4b36",
            b"d2f84c70fa4d85245183efb8390e03a8",
        ),
        ins_op(
            b"48f974117ec1de3898f40dd7d1de774b17dfcc4f3f5b97caab8327a13b51e7e6",
            b"697ff127028aa8c3218abf370957f8f1",
        ),
        ins_op(
            b"336e172aabe748ba306f5527f7b0c808132a6b9bb5c8693aca8c6debdb638ed1",
            b"45038d28c5bafd4209a47003a9d64fa6",
        ),
        ins_op(
            b"47f54faf228c8c7a95a03223db943693d4a64cf0fc3fb45ea0f8daf18034ecbf",
            b"d30b3bfe4307ee2f4031a5fc836289d7",
        ),
        ins_op(
            b"5cb7a321448962d35dfbc168ffc5e372c07879592027571b6d4e6f15f257c871",
            b"68f672a7beaf3d2a131984679a6d9b16",
        ),
        ins_op(
            b"e0ccef093f197747577bfbe4688d06425976dc94c41aff62b4a02ce4cf201585",
            b"167320c01294b05a3ee0335d79e2f28b",
        ),
        ins_op(
            b"3e312123cc657dd53ad9ead5f7d883a199d3ab8fe1ea8f9bf31ab4c332c1d5d3",
            b"4759442ed6afee0a65f1121f0536dbea",
        ),
        ins_op(
            b"2f94e8d04f92deb67ec1cf40fd95111134498a239e860da64f42ac65cf9a4fa0",
            b"57a074f6e979e0684eca779377ac9c20",
        ),
        ins_op(
            b"4dd254290bb99e72cb8ef55c07328dd6e8c09da2460de02dfa20c57c990db64a",
            b"b7b82ec4ae2e7dd045ac127321d4d9c5",
        ),
        ins_op(
            b"a85ad9d6d5b02e425a36eeca897356dea45b0899218cd4fdc549871c060b7875",
            b"1a0ac99e765d076b31a71326d98a12fc",
        ),
        ins_op(
            b"2e36ffa9b0560fb5bd36fe3cc18be9fffab368ae97689db29162a5b25bb8defe",
            b"11ff39bf235e45f5754aa8e6304ccb30",
        ),
        ins_op(
            b"e0370a1237d4d3bd28386115122b12d3eda2b82545d9ada6ea8fdb999901db28",
            b"2e2b9d3f779a1a8740ccb5d02f9f8a42",
        ),
        ins_op(
            b"256098566aab2d640ccf672e7702031ade9b457ca9d77b968bc45d4ebe37f7a4",
            b"9abf1d8934da44465a586d4268f138ee",
        ),
        ins_op(
            b"75dd6e9655c2a170c6d86da6cc2f5f7560e3825cf18f50a12c11b25ef76b1027",
            b"02bd8d8320bde8c0146dadf4082f6cae",
        ),
        ins_op(
            b"19a1b37e6071f158794bd3f71f050f986dc2b20b5f7e2c73349a43ed2fc95632",
            b"a66fcb935a9a6e8901d4d19765b225c0",
        ),
        ins_op(
            b"325e4578fb95d9846b940ee082a9e2742e220ac7b2d2ad33fd22129ff852340d",
            b"fe1e86e1cd7d6fc5063f1c0dba350942",
        ),
        ins_op(
            b"432219d452dbf20bcb0580950de0e8d8f215d581eea13cfdf83a6c52abbc8cfb",
            b"5ace91de860e0bf653eedfe5604193fc",
        ),
        ins_op(
            b"63b426ea793d902b649d92a0e8101cc29bef28f129b04d41ead32d6571f6a86c",
            b"1a78cae401352839223dee36395a37e0",
        ),
        ins_op(
            b"2b75812d6b376bd8ee850e2a9ee9e62e9e183f665377cfc28e0d5a4132e23e78",
            b"5044374f9f47c2fe7e99cd02d78f19a4",
        ),
        ins_op(
            b"d1a913bb6ed0009717864aee5f3724ae1b76c0767e407807d2ca3a074bdb991e",
            b"8747502115327b0b63139c9362a1d232",
        ),
        ins_op(
            b"972af09a09ce7197e75c8f202d35e61fbeda2b5a80fa58ba95ae77dec094af9a",
            b"0d85cb63ad0fa9a350dcefe97b7db1b0",
        ),
        ins_op(
            b"df272eb60444fc1b9c14f52662b10c2c2aba8ca6761f83943c81dfcbd20a6636",
            b"d4f6dffc036582bd659cda3f25adb5f0",
        ),
        ins_op(
            b"4db1cd1ac6a4eb117d2837766727a33a7039e862fb5df7fe0c37dcd22798d0fd",
            b"5ce9de9b38f34a8a3b8f677274c7ccae",
        ),
        ins_op(
            b"26340a0aed14a459bd3c8b4f2e0092736701c35127d0bfc54fc04143a0f68f49",
            b"f4e2b49037db6ec11670e506bf2a4e54",
        ),
        ins_op(
            b"70340cdcad3993558b5da5d759a6f76579f30ce0ae0535758eb853848dfa8038",
            b"0a7c40d8bdef0ba542f6a502728ce005",
        ),
        ins_op(
            b"59009a7346956fbd5f65527b757b79dcd6ede5778cc1d36933e8f8813ebdae6c",
            b"5eeaab358a18c5947e098bcfc698adfc",
        ),
        ins_op(
            b"9373150388beb70d1cb486b453d765c4b882cd5e1fe8c4b0a1da71f8f2bcfffc",
            b"bab707b6ac8ca2774ad680b806ab0f9f",
        ),
        ins_op(
            b"28f68dcdb534e96094e28936cec858d939f0aa21cac2d5bab87fa61357d88196",
            b"964d3ac980d9dfbe3ef2ab1161a81a4f",
        ),
        ins_op(
            b"894c207c3f92fce8087000ff5242326968f0a2f2a07ca821258f6a639a92741b",
            b"b7300e02c9e0bf264b6f68d911b48aeb",
        ),
        ins_op(
            b"ee4248dd48eebbb06c50dfd8eb8cca90ebfe3a040727ec8e1cf47ba5d2d02dcd",
            b"ef2f86e52d43f5d843a232d296abf18c",
        ),
        ins_op(
            b"cb8800b5154e058866873b7c74fee454d7c22d7f22d2923b4d806cf662c200a8",
            b"7994640e21a4c147106719719cce5f7c",
        ),
        ins_op(
            b"05d00696414311d892e9ff87498d9e695ef22547462dba9baa62de1ec82f1752",
            b"e8a9507e195723a64e35fac71a3b9166",
        ),
        ins_op(
            b"0bed223a72824a24eccfe2adadca145cc6b80cb32330eef958cecaffe708f702",
            b"1c5f2e4fe7930d5d46c10c2ac57bbe05",
        ),
        ins_op(
            b"72b6c1b9d942955d5eab90af9346a90241adf0a2d7de88ab30ed43af5023d47f",
            b"944e0c961934e9881ebd8ab16cf8b2fc",
        ),
        ins_op(
            b"06d174e26a3db9d508bf25b5f18ae66fa9100227a71c66abab07c9e2ca304da4",
            b"fc0c861631a2528f7bc5f5b3c266d980",
        ),
        ins_op(
            b"7d0cd821342853406cbf929904a7ab8e214a8df41bec567f62a77f917b487852",
            b"5d9d21ea7d825bee3794c4a7c81b01a3",
        ),
        ins_op(
            b"79317254fb0c25b66f0586f813c1f6ff2164a640b25431f0519c59d2b96d68b0",
            b"6243711ebd9dd51a4a8f37f0f03d17a5",
        ),
        ins_op(
            b"4eee20ea80e093e87f31ea498803e5e32b5ef2576d8f41a78351cabf7322f27c",
            b"45864db9601a18a424ec79d202aa5360",
        ),
        ins_op(
            b"724e88691cca6046ff0f0d167b762127e5758672c35036ff0f8a42061ec3bb63",
            b"dbb0c5dd2e243f5c5398e1cad505feb0",
        ),
        ins_op(
            b"dc876e92edc9d2a9e8bbc6f072110a351513c941a45fe8bfb1097bf524676d78",
            b"c63d47cc5497830c289457426d4aedd9",
        ),
        ins_op(
            b"22e1daa558985ba30ce48214b086b3ad6401962599d52c2b5e8f8b134a8fa9d6",
            b"2006c91ea4f816f30c80667057f632ce",
        ),
        ins_op(
            b"d9861f7a59867d5156bf76566bcfc59300683eab8816211fc336538da94c53ee",
            b"c83fb6f85e44027c13cf79303e0eef43",
        ),
        ins_op(
            b"728c5a9605b43e0eab7c281fccd4d0a42579db51e57ed1543c880782a04aa30e",
            b"7f67517578f38cca4b96ff48f15dcbb7",
        ),
        ins_op(
            b"c64e79c1ccf52f8c8db2095381eac5a24a4693f2919a12bc51250133654d047a",
            b"36397422db755db07c8a7e0c2eed1797",
        ),
        ins_op(
            b"37341f63d44ad9772cabefd63d1c2f3b264897aaf9da51fa5ab539c4876f40fc",
            b"e71c006765fc0d0c5e577a1121270ba2",
        ),
        ins_op(
            b"c99fd09b8875c102d4374ae7ae258548b41a2377921ba593776385254282dd36",
            b"5b7cc052c2af1c5a46dc063b19f6b799",
        ),
    ];

    for m in &in_list {
        prover.perform_one_operation(m).unwrap();
    }
    {
        let rust_proof = prover.generate_proof();
        let path = Path::new("tests/scala_proves/proof1.dmp");
        let mut file = File::open(&path).unwrap();
        let mut scala_proof = Vec::new();
        file.read_to_end(&mut scala_proof).unwrap();
        assert_eq!(rust_proof, &*scala_proof);
    }
    let mods = [
        rem_op(b"5cb7a321448962d35dfbc168ffc5e372c07879592027571b6d4e6f15f257c871"),
        rem_op(b"628181857476ed88dcbf0d77c460b8626cd968a9bdf43c1d2ea67f2acd3694d3"),
        rem_op(b"62c53a6093e9f3dec35db79351227c1fddceea71954a1496ba1b8299f2d27b8b"),
        rem_op(b"63b426ea793d902b649d92a0e8101cc29bef28f129b04d41ead32d6571f6a86c"),
        rem_op(b"63f5e594f305b10bd6d4efa17c7fc64855aa75f4241451d2440e1890c21de5d1"),
        rem_op(b"66566ba505b632ce2fe70973bd2f695f62ca61edf78dd855424e1b80ed9db7ef"),
        ins_op(
            b"f95d8f8958dae7957313842290d6c985e49caf7d5de63085a4edde5b0bd002a9",
            b"56e463c8f3614070131984679a6d9b16",
        ),
        ins_op(
            b"f4290c65a3fdc83c308a420a6a188df42c75ddeb97367458531e9cd820ff8cf6",
            b"03b0c41bc7785b8e4149a49ebad3030a",
        ),
        ins_op(
            b"83c8710c320e5f83243219d034db074fecf07c216bad9720ddaba2c2a355cb0c",
            b"a7b9d102dff1759147417c877696dfe2",
        ),
        ins_op(
            b"87ef0ce10ef152a7710f1d1ebaa900f127d2237780674a0ff1e927c69cc4957f",
            b"752bf9c49718121e223dee36395a37e0",
        ),
        ins_op(
            b"116dadedf0b32272044aa6b4b4440c0e4f340af205ebec098e6c364b89483cae",
            b"6b9dcebee140a24b36091ea96811841d",
        ),
        ins_op(
            b"20fd72b94b85362d4305d4e61e80d17ffdc809bb65c6282851e4af3ea26fb0f3",
            b"fadbbf6d917c0048246c4461975e7fef",
        ),
        rem_op(b"59009a7346956fbd5f65527b757b79dcd6ede5778cc1d36933e8f8813ebdae6c"),
        ins_op(
            b"6600fb2be8bf70ccb25c5f44b6a2f84c37a87143eb42a9dd55ebdebeb37fbc87",
            b"ba4e81b0be31d1157e098bcfc698adfc",
        ),
        rem_op(b"4dd254290bb99e72cb8ef55c07328dd6e8c09da2460de02dfa20c57c990db64a"),
        rem_op(b"4eee20ea80e093e87f31ea498803e5e32b5ef2576d8f41a78351cabf7322f27c"),
        rem_op(b"58b5f141d5b0d4f4c991e8ebdc6e73b3fb661d6c4e29eed688b70850d37cff62"),
        ins_op(
            b"1adfd8f4ebef7085d4e12e8bf5c1b0df2815b42a9fc17e524532367c6663b4a4",
            b"12428bd3c6b90a3945ac127321d4d9c5",
        ),
        ins_op(
            b"5da1d13a2010533691b55e210e15f839c5336e975c55aa433537bbfa7d0788ea",
            b"1ded810ead7d421f24ec79d202aa5360",
        ),
        ins_op(
            b"476588a50ef9f7e7fbe758b44892652590cdac93dc2cb3967deded935615a07f",
            b"efa9215460672b9c59ec1d78e34a65e5",
        ),
        rem_op(b"4db1cd1ac6a4eb117d2837766727a33a7039e862fb5df7fe0c37dcd22798d0fd"),
        ins_op(
            b"13bc02ec628797ae83b27496efcca59e6cba72d4dc25b903f43d643cea7e40b6",
            b"8659eac173f419c93b8f677274c7ccae",
        ),
        rem_op(b"41c55b11a7f61fd17ad228a98195383b5fc084bcc1689839bf4a8f858dbe5390"),
        rem_op(b"432219d452dbf20bcb0580950de0e8d8f215d581eea13cfdf83a6c52abbc8cfb"),
        rem_op(b"47f54faf228c8c7a95a03223db943693d4a64cf0fc3fb45ea0f8daf18034ecbf"),
        rem_op(b"48f974117ec1de3898f40dd7d1de774b17dfcc4f3f5b97caab8327a13b51e7e6"),
        rem_op(b"4a83fefebe881f26974969c3127ebbfe711358cb89465bc3b186f895bcd61f5e"),
        ins_op(
            b"4ffb34e639149a39ee373358ebd20a9d91b3f7436558cc7d93ea7a0faf8cace8",
            b"ec04e5f73a7a30f87b2cd211a5989b21",
        ),
        ins_op(
            b"5cbb132d2e59bc2b4321e74b0f717cdd63139e987b192b050b9355da790cfec3",
            b"7743706723e58fc553eedfe5604193fc",
        ),
        ins_op(
            b"81cdfba25942dc0b8a364f30099ccd89d6bcde41f0a48eb153a258ce43bad62d",
            b"d417b1fa754c8a324031a5fc836289d7",
        ),
        ins_op(
            b"64f4e9275fdebd068bb14658481a6f1dd409225012a89dd27e9583b9d0c0038a",
            b"d10f0f9cbedd3d2a218abf370957f8f1",
        ),
        ins_op(
            b"69151a884bf05b83b32856caad42268bbf783fb5678099178821fc84f49c076f",
            b"4d699f2251f217a7318a65fc169060e5",
        ),
        rem_op(b"3fbbac896a6f6189f29f7440be817332de6f8d2d72f2d55d8c5f2d7a08d9ba4c"),
        rem_op(b"419b1c45fcb6080fe8d091474e71b413974e2848dd1c823c8e809fb1d71f0f23"),
        ins_op(
            b"aa000113afd1e3b12122a3fe583811d4b4c66de094d285f2e8abbfce9f49f859",
            b"fed4138a9cdabf0168034d261756e48a",
        ),
        ins_op(
            b"a7b847359c4e049330b0b9050b83d4bc6776101502166c72aff9960347124bbf",
            b"56412b52851444b92036fa9d4a45cf11",
        ),
        rem_op(b"325e4578fb95d9846b940ee082a9e2742e220ac7b2d2ad33fd22129ff852340d"),
        rem_op(b"333724f1e5ed593ff3760e2fd14257e53320fcaba195198fe364c18317c8357a"),
        rem_op(b"336e172aabe748ba306f5527f7b0c808132a6b9bb5c8693aca8c6debdb638ed1"),
        rem_op(b"369b593ae971613636048143b39d895148aca2eed84f91bba2a03009f2e30c68"),
        rem_op(b"37341f63d44ad9772cabefd63d1c2f3b264897aaf9da51fa5ab539c4876f40fc"),
        rem_op(b"3e312123cc657dd53ad9ead5f7d883a199d3ab8fe1ea8f9bf31ab4c332c1d5d3"),
        rem_op(b"3ee0e1bebd6f4a3b48e45a3036aa13bdf91915066af52ac082c128220072e559"),
        rem_op(b"3eef10eaab9e14d6329a74dde04575747e3b1b10e8f8413d1f474392d3952f88"),
        ins_op(
            b"8154445ab66a5061b7804f259b09e1625e9e9e7adc4543370951b134c02e1a7b",
            b"ca854b7a9e1faf73063f1c0dba350942",
        ),
        ins_op(
            b"f30edcace9bb49125389432092246a0edb2f8de700118177fb2a22d7d0bd5a3f",
            b"bf7608020c18def50f319a10a4166099",
        ),
        ins_op(
            b"24ac6ee3a6d66995b58f760fa17d9c832a48bc696797d641bff45faa345b1482",
            b"ccb6d4ee8da9dcd209a47003a9d64fa6",
        ),
        ins_op(
            b"c233c237611314a789e6ef8665e6787a4506f2fabea785521bb1750affbe6034",
            b"574abdccc173df1e0b1a10783998ff7b",
        ),
        ins_op(
            b"abf1638449815d210f44216133c411e05787359d25171e67496315ebf0705104",
            b"3cb111384825599f5e577a1121270ba2",
        ),
        ins_op(
            b"f15b847f1e548dc27f27ce7a81c233b9d23f339323e33e053655605f63afd7f1",
            b"5e61f0dbaf5215b165f1121f0536dbea",
        ),
        ins_op(
            b"cac6b8babc01acf6d16ce13c19127562b92af53e848f78aa9453d9ec6e399ebb",
            b"22c1070a8f4c58ac2c366789879c5dc2",
        ),
        ins_op(
            b"0c081bc4860cd3dcfd184ab5cf95d8e84404b5baebeeb4369476d95d0ef17302",
            b"342ff6039a7ebd5f3f3b60c517aa742e",
        ),
        rem_op(b"256098566aab2d640ccf672e7702031ade9b457ca9d77b968bc45d4ebe37f7a4"),
        rem_op(b"26340a0aed14a459bd3c8b4f2e0092736701c35127d0bfc54fc04143a0f68f49"),
        rem_op(b"28f68dcdb534e96094e28936cec858d939f0aa21cac2d5bab87fa61357d88196"),
        rem_op(b"2b75812d6b376bd8ee850e2a9ee9e62e9e183f665377cfc28e0d5a4132e23e78"),
        rem_op(b"2dca8ddb1ffbb4e6d25aa9ac83edd73c7c47da9c2c671a3b4f79eb69bf0314ea"),
        rem_op(b"2e36ffa9b0560fb5bd36fe3cc18be9fffab368ae97689db29162a5b25bb8defe"),
        rem_op(b"2f94e8d04f92deb67ec1cf40fd95111134498a239e860da64f42ac65cf9a4fa0"),
        ins_op(
            b"c4daefc8c7d25e534009102b1b9e047fec3fd9a80d55953d171c0ef6d83cb968",
            b"714b51fd89ed203b5a586d4268f138ee",
        ),
        ins_op(
            b"ffba8c2d4ffa8fe4648547ec98009f622c4e6a5cfec05bbf3c12f73a59378616",
            b"25434341db41e9f51670e506bf2a4e54",
        ),
        ins_op(
            b"da4003be59ac3eeccb618c7bff6eff52ed577798976d878b85b8a898f6e30cb3",
            b"8f3ef73098d91fa73ef2ab1161a81a4f",
        ),
        ins_op(
            b"50a14dfca657cf0021a9d232581f311290ae288c570121e0c70666ed55f8ee27",
            b"1511e6de51c02ef57e99cd02d78f19a4",
        ),
        ins_op(
            b"0fc5fc67fb22aaab3ea5661374fd6a2a9f5d7214172ad43fc4f831245dcc72a8",
            b"97950efca92a995243ca3ec7c3bd03d9",
        ),
        ins_op(
            b"e17f658e4fcea87f94b67999e18f8748ae13fc6b7fe7a8cd0b38cd525d24000e",
            b"8fa4f6a6541cb846754aa8e6304ccb30",
        ),
        ins_op(
            b"fa56e0fcfae8b1949df48320fd8384f0e71594c10652c5b597396f92bff91f62",
            b"b42088f81ab1efee4eca779377ac9c20",
        ),
        rem_op(b"06d174e26a3db9d508bf25b5f18ae66fa9100227a71c66abab07c9e2ca304da4"),
        rem_op(b"08df47d9e21a228b56872d187e7ca0033dfe93fc3afa1d1f754d6a40a427524e"),
        rem_op(b"0bed223a72824a24eccfe2adadca145cc6b80cb32330eef958cecaffe708f702"),
        rem_op(b"0d58e0cda79b82eddf8d518d4a1addf5a92447091d78bcecdb727ab0c81d7a7b"),
        rem_op(b"0f09f7704ed285c56154a2f7aef6eadde3e0faf9f3f07bb84393d7f83f8f9669"),
        rem_op(b"15206aba0f8bf92e8ea9d4d9eb2db214c6086da88605180f090024183ca4ec21"),
        rem_op(b"19a1b37e6071f158794bd3f71f050f986dc2b20b5f7e2c73349a43ed2fc95632"),
        rem_op(b"1ea6aac667fc1a99abbb04b45128d8b07b42928e399ec8a4822ab5978373fa14"),
        rem_op(b"22e1daa558985ba30ce48214b086b3ad6401962599d52c2b5e8f8b134a8fa9d6"),
        ins_op(
            b"59682cb496608950676f8354c5a368fd1768ecc8cc54a8b68b70b356f1ab5392",
            b"acc00000811dc0a97bc5f5b3c266d980",
        ),
        ins_op(
            b"8803c2a4a9db10cfa4334b7ea30446f13ab60ce455bb814b10727eea2ce64517",
            b"b26fcd738f5a4050569d6262775712f9",
        ),
        ins_op(
            b"75b7a9253a527b4943eda377d71e5f97461ed021bb82c27c030f0dc2e11ee9c1",
            b"78a239cf569ef80e46c10c2ac57bbe05",
        ),
        ins_op(
            b"089152570def1e5a7b3b895fdcc1596405c7d09ea0a6f30d9cfaa8069fcb52ea",
            b"3d3c68ad26ecc5be64f81f3af4c73aea",
        ),
        ins_op(
            b"a6be39bfcb06978dcdc8ebdd382b29bfe2f533742542574ede57cc6fc862f17a",
            b"0f3f8cbe7accc279779cf2eadb8854ea",
        ),
        ins_op(
            b"a5125cf7eef4f980c6dbcf2ca35fee7b44630994c54583b33c104c0819d8dabf",
            b"033c37235161a21c742fc2160c9336dd",
        ),
        ins_op(
            b"8be351cecbe265ff8a8c0e92f0756963a08cc073647c3bba5e7a85c99208bfd9",
            b"1b40e3f6098534f801d4d19765b225c0",
        ),
        ins_op(
            b"c146dc743368d717d0987322a5bfc6eeb26d591b15b2a87b864621ed3a4ec2a3",
            b"40dc857488c7f0fa13b77f5ef3fe7d3a",
        ),
        ins_op(
            b"4013fbb3c15e12b172505bc4b8a03c47e95e9684d63c7b74a614e7a6bcd3140a",
            b"d2f536410dbc95c60c80667057f632ce",
        ),
        rem_op(b"0362af1eaf3b25f71fb1db4775ae57264e5fc9f933faa3f5bb2012c95d8754cc"),
        rem_op(b"053d23c7067a430081b807b95b96ec914ecb974275785953236eddec3c78aa9c"),
        rem_op(b"05d00696414311d892e9ff87498d9e695ef22547462dba9baa62de1ec82f1752"),
        ins_op(
            b"38f4a80b6061d8d5a719976a7ef4bb9a91abb1042f9a302291cd82de1e76e4f7",
            b"cb704a7656ef281e0b6ea12e9d75c632",
        ),
        ins_op(
            b"415dd03a378ce730475cdbbe0c11c0065483cc73e3d0533cc6b95513d219be57",
            b"f8884f7821d1122d0c1228e344e0fe02",
        ),
        ins_op(
            b"eca5bd7cdd5f27dd988910c17413180ed9dec5195b654fdb4f9627f432c80567",
            b"4d681990c382c28b4e35fac71a3b9166",
        ),
    ];
    for m in &mods {
        prover.perform_one_operation(m).unwrap();
    }

    let to_remove_nodes = prover.removed_nodes();
    {
        let rust_proof = prover.generate_proof();
        let path = Path::new("tests/scala_proves/proof2.dmp");
        let mut file = File::open(&path).unwrap();
        let mut scala_proof = Vec::new();
        file.read_to_end(&mut scala_proof).unwrap();
        assert_eq!(rust_proof, &*scala_proof);
    }
    for rn in &to_remove_nodes {
        assert!(!prover.contains(rn));
    }
}

fn visit_nodes(node: &NodeId, visited: &mut Vec<NodeId>) {
    if node.borrow().is_new() {
        visited.push(node.clone());
        match &*node.borrow() {
            Node::Internal(i) => {
                visit_nodes(&i.left, visited);
                visit_nodes(&i.right, visited);
            }
            _ => {}
        }
    }
}

fn visited_nodes(node: &NodeId) -> Vec<NodeId> {
    let mut visited: Vec<NodeId> = Vec::new();
    visit_nodes(node, &mut visited);
    visited
}

#[test]
fn test_removed_nodes_and_new_nodes() {
    // removedNodes() should not contain new nodes
    let mut prover = generate_and_populate_prover(INITIAL_TREE_SIZE).0;

    for _ in 0..TEST_ITERATIONS {
        let kv_list = random_kv_list(MAX_LIST_SIZE);
        let m_size = kv_list.len();
        let to_insert = kv_list.iter().map(|kv| Operation::Insert(kv.clone()));
        let to_remove: Vec<Operation> = (0..m_size)
            .flat_map(|i| prover.random_walk(&mut StdRng::seed_from_u64(i as u64)))
            .map(|kv| Operation::Remove(kv.key))
            .collect();
        let modifications = to_insert.chain(to_remove);
        for op in modifications {
            let _ = prover.perform_one_operation(&op);
        }
        let removed = prover.removed_nodes();
        let new_nodes = visited_nodes(&prover.top_node());
        new_nodes.iter().for_each(|nn| {
            assert!(removed
                .iter()
                .find(|r| {
                    let l1 = r.borrow_mut().label();
                    let l2 = nn.borrow_mut().label();
                    l1 == l2
                })
                .is_none())
        });

        prover.generate_proof();
    }
}

#[test]
fn test_return_removed_nodes() {
    // return removed leafs and internal nodes
    let mut prover = generate_and_populate_prover(INITIAL_TREE_SIZE).0;
    for _ in 0..TEST_ITERATIONS {
        let old_top = prover.top_node();
        let kv_list = random_kv_list(MAX_LIST_SIZE);
        let m_size = kv_list.len();
        let to_insert = kv_list.iter().map(|kv| Operation::Insert(kv.clone()));
        let to_remove: Vec<Operation> = (0..m_size)
            .flat_map(|i| prover.random_walk(&mut StdRng::seed_from_u64(i as u64)))
            .map(|kv| Operation::Remove(kv.key))
            .collect();
        let modifications = to_insert.chain(to_remove.clone());
        modifications.for_each(|op| {
            let _ = prover.perform_one_operation(&op);
        });

        let removed = prover.removed_nodes();
        assert!(removed.len() > m_size);
        to_remove.iter().for_each(|tr| {
            assert!(removed
                .iter()
                .find(|node| node.borrow().key() == tr.key())
                .is_some())
        });
        check_tree(&mut prover, &old_top, &removed);

        let _modifying_proof = prover.generate_proof();
        assert!(prover.removed_nodes().is_empty());
    }
}

#[test]
fn test_proof_generation_special_case() {
    // proof generation without tree modification special case
    let start_tree_size: usize = 82;
    let to_remove_size: usize = 1;
    let (mut prover, elements) = generate_and_populate_prover(start_tree_size);

    let mods: Vec<Operation> = elements[..to_remove_size]
        .iter()
        .map(|kv| Operation::Remove(kv.key.clone()))
        .collect();

    let (non_modifying_proof, _non_modifying_digest) =
        prover.generate_proof_for_operations(&mods).unwrap();

    mods.iter()
        .for_each(|op| assert!(prover.perform_one_operation(op).is_ok()));

    let _ = prover.removed_nodes();

    let proof_bytes = prover.generate_proof();

    assert_eq!(non_modifying_proof, proof_bytes);
}

#[test]
fn test_proof_generation() {
    // proof generation without tree modification
    let mut prover = generate_and_populate_prover(INITIAL_TREE_SIZE).0;
    for _ in 0..TEST_ITERATIONS {
        let kv_list = random_kv_list(MAX_LIST_SIZE);
        let insert_num = std::cmp::min(10, kv_list.len());
        let to_insert: Vec<Operation> = kv_list[..insert_num]
            .iter()
            .map(|kv| Operation::Insert(kv.clone()))
            .collect();
        let to_remove: Vec<Operation> = (0..insert_num)
            .flat_map(|i| prover.random_walk(&mut StdRng::seed_from_u64(i as u64)))
            .map(|kv| Operation::Remove(kv.key))
            .collect();
        let mut modifications = to_insert.clone();
        modifications.extend_from_slice(&to_remove);
        let initial_digest = prover.digest().unwrap();

        // generate proof without tree modification
        let (non_modifying_proof, non_modifying_digest) = prover
            .generate_proof_for_operations(&modifications)
            .unwrap();
        assert_eq!(prover.digest().unwrap(), initial_digest);
        to_insert
            .iter()
            .for_each(|ti| assert!(prover.unauthenticated_lookup(&ti.key()).is_none()));
        to_remove
            .iter()
            .for_each(|ti| assert!(prover.unauthenticated_lookup(&ti.key()).is_some()));
        let mut verifier = generate_verifier(
            &initial_digest,
            &non_modifying_proof,
            KEY_LENGTH,
            None,
            None,
            None,
        );
        modifications
            .iter()
            .for_each(|m| assert!(verifier.perform_one_operation(m).is_ok()));
        assert_eq!(verifier.digest().unwrap(), non_modifying_digest);

        // modify tree and generate proof
        modifications
            .iter()
            .for_each(|ti| assert!(prover.perform_one_operation(ti).is_ok()));
        let _ = prover.removed_nodes();
        let modifying_proof = prover.generate_proof();
        assert_eq!(prover.digest(), verifier.digest());
        assert_ne!(prover.digest().unwrap(), initial_digest);
        assert_eq!(modifying_proof, non_modifying_proof);
        to_insert
            .iter()
            .for_each(|ti| assert_eq!(prover.unauthenticated_lookup(&ti.key()), ti.value()));
        to_remove
            .iter()
            .for_each(|ti| assert!(prover.unauthenticated_lookup(&ti.key()).is_none()));
    }
}

#[test]
fn test_random_walk() {
    let prover = generate_and_populate_prover(INITIAL_TREE_SIZE).0;
    for _ in 0..TEST_ITERATIONS {
        let seed: u64 = rand::thread_rng().gen();
        let e1 = prover.random_walk(&mut StdRng::seed_from_u64(seed));
        let e2 = prover.random_walk(&mut StdRng::seed_from_u64(seed));
        assert_eq!(e1, e2);
    }
}

fn i2b(i: u64) -> Bytes {
    Bytes::copy_from_slice(&i.to_be_bytes())
}

fn s2b(slice: &[u8]) -> Bytes {
    Bytes::copy_from_slice(slice)
}

#[test]
fn test_unauthenticated_lookup() {
    let mut p = generate_prover(8, None);
    assert!(p
        .perform_one_operation(&Operation::Insert(KeyValue {
            key: i2b(1u64),
            value: s2b(&[0u8; 4])
        }))
        .is_ok());
    assert!(p
        .perform_one_operation(&Operation::Insert(KeyValue {
            key: i2b(2u64),
            value: s2b(&[0u8; 5])
        }))
        .is_ok());
    assert!(p
        .perform_one_operation(&Operation::Insert(KeyValue {
            key: i2b(3u64),
            value: s2b(&[0u8; 6])
        }))
        .is_ok());
    assert!(p
        .perform_one_operation(&Operation::Insert(KeyValue {
            key: i2b(4u64),
            value: s2b(&[0u8; 7])
        }))
        .is_ok());
    assert!(p
        .perform_one_operation(&Operation::Insert(KeyValue {
            key: i2b(5u64),
            value: s2b(&[0u8; 8])
        }))
        .is_ok());
    assert!(p
        .perform_one_operation(&Operation::Insert(KeyValue {
            key: i2b(6u64),
            value: s2b(&[0u8; 9])
        }))
        .is_ok());

    assert!(p.unauthenticated_lookup(&i2b(0u64)).is_none());
    assert_eq!(p.unauthenticated_lookup(&i2b(1u64)).unwrap().len(), 4);
    assert_eq!(p.unauthenticated_lookup(&i2b(2u64)).unwrap().len(), 5);
    assert_eq!(p.unauthenticated_lookup(&i2b(3u64)).unwrap().len(), 6);
    assert_eq!(p.unauthenticated_lookup(&i2b(4u64)).unwrap().len(), 7);
    assert_eq!(p.unauthenticated_lookup(&i2b(5u64)).unwrap().len(), 8);
    assert_eq!(p.unauthenticated_lookup(&i2b(6u64)).unwrap().len(), 9);
    assert!(p.unauthenticated_lookup(&i2b(7u64)).is_none());
}

#[test]
fn test_extract_nodes() {
    // BatchAVLVerifier: extractNodes and extractFirstNode
    let mut prover = generate_prover(KEY_LENGTH, None);
    let digest = prover.digest().unwrap();
    let key_values = generate_kv_list(INITIAL_TREE_SIZE);
    assert!(key_values.len() == INITIAL_TREE_SIZE);
    key_values.iter().for_each(|kv| {
        assert!(prover
            .perform_one_operation(&Operation::Insert(kv.clone()))
            .is_ok())
    });

    let pf = prover.generate_proof();

    let mut verifier = generate_verifier(&digest, &pf, KEY_LENGTH, None, None, None);

    let infinity_leaf = verifier
        .extract_first_node(&mut |n| match n {
            Node::Leaf(_) => true,
            _ => false,
        })
        .unwrap();

    key_values.iter().for_each(|kv| {
        assert!(verifier
            .perform_one_operation(&Operation::Insert(kv.clone()))
            .is_ok())
    });

    let infinity_label = infinity_leaf.borrow_mut().label();
    let mut non_infinite_leaf = |n: &mut Node| match n {
        Node::Leaf(_) => n.label() != infinity_label,
        _ => false,
    };

    //extract all leafs
    let all_leafs = verifier.extract_nodes(&mut non_infinite_leaf);
    assert_eq!(all_leafs.unwrap().len(), INITIAL_TREE_SIZE);

    //First extracted leaf should be smallest
    let smallest_key = key_values.iter().map(|kv| kv.key.clone()).min().unwrap();
    let min_leaf = verifier.extract_first_node(&mut non_infinite_leaf).unwrap();
    assert_eq!(min_leaf.borrow().key(), smallest_key);
}

#[test]
fn test_verfier_extract_first_node() {
    let mut prover = generate_and_populate_prover(0).0;
    let digest = prover.digest().unwrap();
    let key_values = generate_kv_list(INITIAL_TREE_SIZE);
    key_values.iter().for_each(|kv| {
        assert!(prover
            .perform_one_operation(&Operation::Insert(kv.clone()))
            .is_ok())
    });

    let pf = prover.generate_proof();
    let mut verifier = generate_verifier(&digest, &pf, KEY_LENGTH, None, None, None);

    let infinity_leaf = verifier
        .extract_first_node(&mut |n| match n {
            Node::Leaf(_) => true,
            _ => false,
        })
        .unwrap();

    key_values.iter().for_each(|kv| {
        assert!(verifier
            .perform_one_operation(&Operation::Insert(kv.clone()))
            .is_ok())
    });
    let infinity_label = infinity_leaf.borrow_mut().label();
    let mut non_infinite_leaf = |n: &mut Node| match n {
        Node::Leaf(_) => n.label() != infinity_label,
        _ => false,
    };

    //First extracted leaf should be smallest
    let smallest_key = key_values.iter().map(|kv| kv.key.clone()).min().unwrap();
    let min_leaf = verifier.extract_first_node(&mut non_infinite_leaf).unwrap();
    assert_eq!(min_leaf.borrow().key(), smallest_key);

    //Test every leaf is extractable by key
    key_values.iter().for_each(|kv| {
        let node = verifier.extract_first_node(&mut |node| match node {
            Node::Leaf(l) => *l.hdr.key.as_ref().unwrap() == kv.key,
            _ => false,
        });
        let node = node.unwrap();
        let node = node.borrow();
        assert_eq!(node.key(), kv.key);
        assert_eq!(node.value(), kv.value);
    });

    //False predicate make it return None
    assert!(verifier.extract_first_node(&mut |_| false).is_none());
}

#[test]
fn test_batch_of_lookups() {
    //prepare tree
    let mut prover = generate_and_populate_prover(INITIAL_TREE_SIZE).0;
    let digest = prover.digest().unwrap();

    for number_of_lookups in 0..20 {
        let current_mods: Vec<Operation> = (0..number_of_lookups)
            .map(|_| Operation::Lookup(random_key()))
            .collect();

        current_mods
            .iter()
            .for_each(|m| assert!(prover.perform_one_operation(&m).is_ok()));
        let pf = prover.generate_proof();

        let mut verifier = generate_verifier(&digest, &pf, KEY_LENGTH, None, None, None);
        current_mods
            .iter()
            .for_each(|m| assert!(verifier.perform_one_operation(m).is_ok()));
        assert_eq!(prover.digest(), verifier.digest());
    }
    prover.check_tree(true);
}

#[test]
fn test_tree_varlen_key() {
    // Tree without fixed value length
    let mut prover = generate_prover(KEY_LENGTH, None);
    let mut digest = prover.digest().unwrap();

    for _ in 0..TEST_ITERATIONS {
        let value_length = rand::thread_rng().gen_range(0..0x8000);
        let key = random_key();
        let value = Bytes::from(vec![0u8; value_length]);
        let current_mods = [Operation::Insert(KeyValue { key, value })];

        current_mods
            .iter()
            .for_each(|m| assert!(prover.perform_one_operation(m).is_ok()));
        let pf = prover.generate_proof();

        let mut verifier = generate_verifier(&digest, &pf, KEY_LENGTH, None, None, None);
        current_mods
            .iter()
            .for_each(|m| assert!(verifier.perform_one_operation(m).is_ok()));
        digest = verifier.digest().unwrap();
        assert_eq!(prover.digest().unwrap(), digest);
    }
    prover.check_tree(true);
}

#[test]
fn test_modifications() {
    // Modifications for different key and value length
    for key_length in 1..KEY_LENGTH {
        for value_length in 1..VALUE_LENGTH {
            let mut prover = generate_prover(key_length, Some(value_length));
            let key = Bytes::copy_from_slice(&random_key_with_len(key_length));
            let value = Bytes::copy_from_slice(&random_key_with_len(value_length));
            let m = Operation::Insert(KeyValue {
                key: key.clone(),
                value: value.clone(),
            });
            let digest = prover.digest().unwrap();
            assert!(prover.perform_one_operation(&m).is_ok());
            let pf = prover.generate_proof();

            let mut verifier =
                generate_verifier(&digest, &pf, key_length, Some(value_length), None, None);
            assert!(verifier.perform_one_operation(&m).is_ok());
            if verifier.digest().is_none() {
                println!("problematic key {:?}", key);
                println!("problematic value {:?}", value);
            }
            assert!(verifier.digest().is_some());
            assert_eq!(prover.digest(), verifier.digest());

            let lookup = Operation::Lookup(key.clone());
            assert!(prover.perform_one_operation(&lookup).is_ok());
            let pr = prover.generate_proof();
            let mut vr = generate_verifier(
                &prover.digest().unwrap(),
                &pr,
                key_length,
                Some(value_length),
                None,
                None,
            );
            assert_eq!(vr.perform_one_operation(&lookup).unwrap().unwrap(), value);

            let mut unexisted_key: ADKey;
            loop {
                unexisted_key = Bytes::copy_from_slice(&random_key_with_len(key_length));
                if unexisted_key != key {
                    break;
                }
            }
            let non_existing_lookup = Operation::Lookup(unexisted_key);
            let _ = prover.perform_one_operation(&non_existing_lookup);
            let pr2 = prover.generate_proof();
            let mut vr2 = generate_verifier(
                &prover.digest().unwrap(),
                &pr2,
                key_length,
                Some(value_length),
                None,
                None,
            );
            assert!(vr2
                .perform_one_operation(&non_existing_lookup)
                .unwrap()
                .is_none());
        }
    }
}

#[test]
fn test_lookups() {
    let mut prover = generate_prover(KEY_LENGTH, Some(VALUE_LENGTH));
    for _ in 0..TEST_ITERATIONS {
        let kv_list = random_kv_list(MAX_LIST_SIZE);
        let insert_num = std::cmp::min(3, kv_list.len());
        let to_insert = &kv_list[..insert_num];
        to_insert.iter().for_each(|kv| {
            assert!(prover
                .perform_one_operation(&Operation::Insert(kv.clone()))
                .is_ok())
        });
        prover.generate_proof();
        let lookups = kv_list.iter().map(|kv| Operation::Lookup(kv.key.clone()));

        lookups.for_each(|l| assert!(prover.perform_one_operation(&l).is_ok()));
        let pr = prover.generate_proof();
        let mut vr = generate_verifier(
            &prover.digest().unwrap(),
            &pr,
            KEY_LENGTH,
            Some(VALUE_LENGTH),
            None,
            None,
        );
        kv_list.iter().for_each(|kv| {
            match vr
                .perform_one_operation(&Operation::Lookup(kv.key.clone()))
                .unwrap()
            {
                Some(v) => assert_eq!(to_insert.iter().find(|i| i.key == kv.key).unwrap().value, v),
                None => assert!(to_insert.iter().find(|i| i.key == kv.key).is_none()),
            }
        });
    }
}

#[test]
fn test_authenticated_set() {
    let mut prover = generate_prover(KEY_LENGTH, Some(0));
    let mut digest = prover.digest().unwrap();

    for _ in 0..TEST_ITERATIONS {
        let key = random_key();
        let m = Operation::Insert(KeyValue {
            key,
            value: Bytes::new(),
        });
        assert!(prover.perform_one_operation(&m).is_ok());
        let pf = prover.generate_proof();
        let _ = prover.digest();
        let mut verifier = generate_verifier(&digest, &pf, KEY_LENGTH, Some(0), None, None);
        assert!(verifier.perform_one_operation(&m).is_ok());
        digest = verifier.digest().unwrap();
        assert_eq!(prover.digest().unwrap(), digest);
    }
}

pub fn test_long_update() {
    let mut prover = generate_prover(KEY_LENGTH, Some(VALUE_LENGTH));
    let mut digest = prover.digest().unwrap();

    for _ in 0..TEST_ITERATIONS {
        let kv = random_kv();
        let old_value: i64 = prover
            .unauthenticated_lookup(&kv.key)
            .map(|b| i64_from_bytes(&b))
            .unwrap_or(0i64);
        let delta = i64_from_bytes(&kv.value);

        if old_value.wrapping_add(delta) > old_value {
            let m = Operation::UpdateLongBy(KeyDelta {
                key: kv.key.clone(),
                delta,
            });
            assert_eq!(
                prover
                    .perform_one_operation(&m)
                    .unwrap()
                    .map(|b| i64_from_bytes(&b))
                    .unwrap_or(0i64),
                old_value
            );
            let pf = prover.generate_proof();
            let mut verifier =
                generate_verifier(&digest, &pf, KEY_LENGTH, Some(VALUE_LENGTH), None, None);
            assert!(verifier.perform_one_operation(&m).is_ok());
            digest = verifier.digest().unwrap();
            assert_eq!(prover.digest().unwrap(), digest);
            match prover.unauthenticated_lookup(&kv.key) {
                Some(v) => assert_eq!(delta + old_value, i64_from_bytes(&v)),
                None => assert_eq!(delta + old_value, 0i64),
            }
        }
    }
    prover.check_tree(true);
}

#[test]
fn test_zero_mods() {
    //  property("zero-mods verification on empty tree") {
    let mut p = generate_prover(KEY_LENGTH, Some(VALUE_LENGTH));
    p.check_tree(false);
    let digest = p.digest().unwrap();
    let pf = p.generate_proof();
    p.check_tree(true);
    let v = generate_verifier(
        &digest,
        &pf,
        KEY_LENGTH,
        Some(VALUE_LENGTH),
        Some(0),
        Some(0),
    );
    assert_eq!(v.digest().unwrap(), digest);
}

#[test]
fn test_verifier_fails() {
    // various verifier fails
    let mut p = generate_prover(KEY_LENGTH, Some(VALUE_LENGTH));
    p.check_tree(false);
    for _ in 0..1000 {
        assert!(p
            .perform_one_operation(&Operation::Insert(random_kv()))
            .is_ok());
        p.check_tree(false);
    }
    p.generate_proof();

    let mut digest = p.digest().unwrap();
    for _ in 0..50 {
        assert!(p
            .perform_one_operation(&Operation::Insert(random_kv()))
            .is_ok());
    }
    let mut pf = p.generate_proof();

    // see if the proof for 50 mods will be allowed when we permit only 2
    let v = BatchAVLVerifier::new(
        &digest,
        &pf,
        generate_tree(KEY_LENGTH, Some(VALUE_LENGTH)),
        Some(2),
        Some(0),
    );
    assert!(v.is_err()); // Failed to reject too long a proof"

    // see if wrong digest will be allowed
    let rnd = rand::random::<[u8; KEY_LENGTH]>();
    let v = BatchAVLVerifier::new(
        &Bytes::copy_from_slice(&rnd[..]),
        &pf,
        generate_tree(KEY_LENGTH, Some(VALUE_LENGTH)),
        Some(50),
        Some(0),
    );
    assert!(v.is_err()); // Failed to reject wrong digest

    for _ in 0..10 {
        digest = p.digest().unwrap();
        for _ in 0..8 {
            assert!(p
                .perform_one_operation(&Operation::Insert(random_kv()))
                .is_ok()); // failed to insert
        }
        let mut v = generate_verifier(
            &digest,
            &p.generate_proof(),
            KEY_LENGTH,
            Some(VALUE_LENGTH),
            Some(8),
            Some(0),
        );
        assert!(v.digest().is_some()); // verification failed to construct tree
                                       // Try 5 inserts that do not match -- with overwhelming probability one of them will go to a leaf
                                       // that is not in the conveyed tree, and verifier will complain
        for _ in 0..5 {
            let key = random_key();
            assert!(v
                .perform_one_operation(&Operation::Insert(KeyValue {
                    key: key.clone(),
                    value: random_value()
                }))
                .is_err());
        }
        assert!(v.digest().is_none()); // verification succeeded when it should have failed, because of a missing leaf

        digest = p.digest().unwrap();
        let kv = random_kv();
        let key = kv.key.clone();
        assert!(p.perform_one_operation(&Operation::Insert(kv)).is_ok());
        pf = p.generate_proof();
        p.check_tree(false);

        // Change the direction of the proof and make sure verifier fails
        let mut vpf = pf.to_vec();
        *vpf.last_mut().unwrap() = !vpf.last().unwrap();
        v = generate_verifier(
            &digest,
            &Bytes::copy_from_slice(&vpf),
            KEY_LENGTH,
            Some(VALUE_LENGTH),
            Some(1),
            Some(0),
        );
        assert!(v.digest().is_some()); // verification failed to construct tree
        assert!(v
            .perform_one_operation(&Operation::Insert(KeyValue {
                key: key.clone(),
                value: random_value()
            }))
            .is_err());
        assert!(v.digest().is_none()); // verification succeeded when it should have failed, because of the wrong direction

        // Change the key by a large amount -- verification should fail with overwhelming probability
        // because there are 1000 keys in the tree
        // First, change the proof back to be correct
        *vpf.last_mut().unwrap() = !vpf.last().unwrap();
        let mut vk = key.to_vec();
        vk[0] ^= 1u8 << 7;
        let key = Bytes::copy_from_slice(&vk);
        v = generate_verifier(
            &digest,
            &Bytes::copy_from_slice(&vpf),
            KEY_LENGTH,
            Some(VALUE_LENGTH),
            Some(1),
            Some(0),
        );
        assert!(v.digest().is_some()); // verification failed to construct tree
        assert!(v
            .perform_one_operation(&Operation::Insert(KeyValue {
                key,
                value: random_value()
            }))
            .is_err());
        assert!(v.digest().is_none()); // verification succeeded when it should have failed because of the wrong key
                                       // put the key back the way it should be, because otherwise it's messed up in the prover tree
    }
}

#[test]
fn test_remove_single_random_element() {
    // remove single random element from a large set
    const MIN_SET_SIZE: usize = 10000;
    const MAX_SET_SIZE: usize = 100000;
    let mut generate_proof = true;

    for _ in 0..TEST_ITERATIONS {
        let cnt = rand::thread_rng().gen_range(MIN_SET_SIZE..MAX_SET_SIZE);
        generate_proof = !generate_proof;

        let mut keys: Vec<ADKey> = Vec::new();
        let mut prover = generate_prover(KEY_LENGTH, Some(VALUE_LENGTH));

        for _ in 1..=cnt {
            let kv = random_kv();
            let key = kv.key.clone();
            keys.push(key.clone());

            assert!(prover.perform_one_operation(&Operation::Insert(kv)).is_ok());
            assert!(prover.unauthenticated_lookup(&key).is_some());
        }

        if generate_proof {
            prover.generate_proof();
        }

        let key_position = rand::thread_rng().gen_range(0..keys.len());
        let rnd_key = keys[key_position].clone();

        assert!(prover.unauthenticated_lookup(&rnd_key).is_some());
        let removal_result = prover.perform_one_operation(&Operation::Remove(rnd_key.clone()));
        assert!(removal_result.is_ok());

        if key_position > 0 {
            assert!(prover
                .perform_one_operation(&Operation::Remove(keys.first().unwrap().clone()))
                .is_ok());
        }

        keys.remove(0);
        keys.retain(|x| *x != rnd_key);
        for _ in 0..keys.len() {
            let i = rand::thread_rng().gen_range(0..keys.len());
            let _ = prover.perform_one_operation(&Operation::Remove(keys[i].clone()));
        }
    }
}

#[test]
fn test_successful_modifications() {
    let mut p = generate_prover(KEY_LENGTH, Some(VALUE_LENGTH));

    const NUM_MODS: usize = 5000;

    let mut deleted_keys: Vec<ADKey> = Vec::new();
    let mut keys_and_vals: Vec<KeyValue> = Vec::new();

    let mut i = 0;
    let mut _num_inserts = 0;
    let mut _num_modifies = 0;
    let mut _num_deletes = 0;
    let mut _num_non_deletes = 0;
    let mut _num_failures = 0;
    let mut rnd = rand::thread_rng();

    while i < NUM_MODS {
        let digest = p.digest().unwrap();
        let n = rnd.gen_range(0..100);
        let j = i + n;
        let mut num_current_deletes = 0;
        let mut current_mods: Vec<Operation> = Vec::new();
        while i < j {
            if keys_and_vals.is_empty() || rnd.gen_range(0..2) == 0 {
                // with prob .5 insert a new one, with prob .5 update or delete an existing one
                if !keys_and_vals.is_empty() && rnd.gen_range(0..10) == 0 {
                    // with probability 1/10 cause a fail by inserting already existing
                    let index = rnd.gen_range(0..keys_and_vals.len());
                    let key = keys_and_vals[index].key.clone();
                    let kv = KeyValue {
                        key: key.clone(),
                        value: random_value(),
                    };
                    assert!(p.perform_one_operation(&Operation::Insert(kv)).is_err()); // prover succeeded on inserting a value that's already in tree
                    p.check_tree(false);
                    assert_eq!(
                        p.unauthenticated_lookup(&key).unwrap(),
                        keys_and_vals[index].value
                    ); //  value changed after duplicate insert
                    _num_failures += 1;
                } else {
                    let kv = random_kv();
                    keys_and_vals.push(kv.clone());
                    let op = Operation::Insert(kv.clone());
                    current_mods.push(op.clone());
                    assert!(p.perform_one_operation(&op).is_ok()); // prover failed to insert
                    p.check_tree(false);
                    assert_eq!(p.unauthenticated_lookup(&kv.key).unwrap(), kv.value); // inserted key is missing
                    _num_inserts += 1;
                }
            } else {
                // with probability .25 update, with .25 delete
                if rnd.gen_range(0..2) == 0 {
                    // update
                    if rnd.gen_range(0..10) == 0 {
                        // with probability 1/10 cause a fail by modifying a non-existing key
                        let kv = random_kv();
                        let key = kv.key.clone();
                        assert!(p.perform_one_operation(&Operation::Update(kv)).is_err()); // prover updated a nonexistent value
                        p.check_tree(false);
                        assert!(p.unauthenticated_lookup(&key).is_none()); // a nonexistent value appeared after an update
                        _num_failures += 1;
                    } else {
                        let index = rnd.gen_range(0..keys_and_vals.len());
                        let key = keys_and_vals[index].key.clone();
                        let kv = KeyValue {
                            key: key.clone(),
                            value: random_value(),
                        };
                        let op = Operation::Update(kv.clone());
                        current_mods.push(op.clone());
                        p.perform_one_operation(&op).unwrap();
                        keys_and_vals[index] = kv.clone();
                        assert_eq!(p.unauthenticated_lookup(&key).unwrap(), kv.value); // wrong value after update
                        _num_modifies += 1;
                    }
                } else {
                    // delete
                    if rnd.gen_range(0..10) == 0 {
                        // with probability 1/10 remove a non-existing one but without failure -- shouldn't change the tree
                        let key = random_key();
                        let op = Operation::RemoveIfExists(key);
                        let d = p.digest();
                        current_mods.push(op.clone());
                        assert!(p.perform_one_operation(&op).is_ok()); // prover failed when it should have done nothing
                        assert_eq!(d, p.digest()); // Tree changed when it shouldn't have
                        p.check_tree(false);
                        _num_non_deletes += 1;
                    } else {
                        // remove an existing key
                        let index = rnd.gen_range(0..keys_and_vals.len());
                        let key = keys_and_vals[index].key.clone();
                        let op = Operation::Remove(key.clone());
                        current_mods.push(op.clone());
                        assert!(p.perform_one_operation(&op).is_ok()); // failed ot delete
                        keys_and_vals.remove(index);
                        deleted_keys.push(key.clone());
                        assert!(p.unauthenticated_lookup(&key).is_none()); // deleted key still in tree
                        _num_deletes += 1;
                        num_current_deletes += 1;
                    }
                }
            }
            i += 1;
        }

        let pf = p.generate_proof();
        p.check_tree(true);
        let mut v = generate_verifier(
            &digest,
            &pf,
            KEY_LENGTH,
            Some(VALUE_LENGTH),
            Some(n),
            Some(num_current_deletes),
        );
        assert_eq!(v.digest().unwrap(), digest); // Built tree with wrong digest
        for m in current_mods {
            assert!(v.perform_one_operation(&m).is_ok());
        }
        assert_eq!(v.digest(), p.digest()); // Tree has wrong digest after verification
    }

    // Check that all the inserts, deletes, and updates we did actually stayed
    deleted_keys
        .iter()
        .for_each(|k| assert!(p.unauthenticated_lookup(k).is_none())); // "Key that was deleted is still in the tree
    keys_and_vals
        .iter()
        .for_each(|kv| assert_eq!(p.unauthenticated_lookup(&kv.key).unwrap(), kv.value));
    // Key has wrong value
}

#[test]
fn test_persistence() {
    // "Persistence AVL batch prover
    let storage = Box::new(VersionedAVLStorageMock::new());
    let p = generate_prover(KEY_LENGTH, Some(VALUE_LENGTH));
    let mut prover = PersistentBatchAVLProver::new(p, storage, Vec::new()).unwrap();
    let mut digest = prover.digest();

    for _ in 0..TEST_ITERATIONS {
        let kv = random_kv();
        let m = Operation::Insert(kv);
        assert!(prover.perform_one_operation(&m).is_ok());
        let pf = prover
            .generate_proof_and_update_storage(Vec::new())
            .unwrap();

        let mut verifier =
            generate_verifier(&digest, &pf, KEY_LENGTH, Some(VALUE_LENGTH), None, None);
        verifier.digest().unwrap();
        verifier.perform_one_operation(&m).unwrap();

        assert_ne!(prover.digest(), digest);
        assert_eq!(prover.digest(), verifier.digest().unwrap());

        assert!(prover.rollback(&digest).is_ok());
        assert_eq!(prover.digest(), digest);
        assert!(prover.perform_one_operation(&m).is_ok());
        assert!(prover.generate_proof_and_update_storage(Vec::new()).is_ok());
        digest = prover.digest();
    }

    let digest = prover.digest();
    let prover2 = PersistentBatchAVLProver::new(
        generate_prover(KEY_LENGTH, Some(VALUE_LENGTH)),
        prover.storage,
        Vec::new(),
    )
    .unwrap();
    assert_eq!(prover2.digest(), digest);
}

#[test]
fn test_verifier_calculate_same_digest() {
    let mut prover = generate_prover(KEY_LENGTH, Some(VALUE_LENGTH));
    let mut digest = prover.digest().unwrap();

    for _ in 0..TEST_ITERATIONS {
        let kv = random_kv();
        let op = &Operation::Insert(kv);

        assert!(prover.perform_one_operation(&op).is_ok());
        let pf = prover.generate_proof();

        let mut verifier =
            generate_verifier(&digest, &pf, KEY_LENGTH, Some(VALUE_LENGTH), None, None);
        assert!(verifier.perform_one_operation(&op).is_ok());
        digest = verifier.digest().unwrap();
        assert_eq!(digest, prover.digest().unwrap());
    }
    prover.check_tree(true);
}

#[test]
fn remove_nodes_benchmark() {
    let start_tree_size: usize = 100000;
    let iterations: usize = 100;
    let to_remove_size: usize = 1000;
    let to_insert_size: usize = 1000;
    let (mut prover, elements) = generate_and_populate_prover(start_tree_size);
    let now = Instant::now();

    for i in 0..iterations {
        let mut to_remove: Vec<Operation> = elements[i * to_remove_size..(i + 1) * to_remove_size]
            .iter()
            .map(|kv| Operation::Remove(kv.key.clone()))
            .collect();
        let to_insert: Vec<Operation> = (0..to_insert_size)
            .map(|j| {
                let k = sha256(&format!("{}-{}", i, j));
                Operation::Insert(KeyValue {
                    key: k.clone(),
                    value: Bytes::copy_from_slice(&k[..8]),
                })
            })
            .collect();

        let mut mods = to_insert;
        mods.append(&mut to_remove);

        let non_modifying_proof = prover.generate_proof_for_operations(&mods).unwrap().0;
        mods.iter()
            .for_each(|op| assert!(prover.perform_one_operation(op).is_ok()));

        let removed_nodes = prover.removed_nodes();
        let _removed_nodes_length = removed_nodes.len();

        let proof_bytes = prover.generate_proof();
        assert_eq!(non_modifying_proof, proof_bytes);
    }
    println!("Elapsed time: {:?}", now.elapsed());
}
