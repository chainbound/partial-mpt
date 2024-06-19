use alloy_eips::{BlockId, BlockNumberOrTag};
use alloy_primitives::{Address, B256, U256};
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::BlockTransactionsKind;
use alloy_transport_http::reqwest::Url;
use dotenvy::dotenv;
use partial_mpt::StateTrie;

#[tokio::main]
async fn main() {
    // gm, u know how the ethereum's state root would look like if all ETH on zero address is burnt?!
    //
    // if this api key doesn't work pls use your own
    dotenv().ok();
    let api_key = std::env::var("ALCHEMY_API_KEY").unwrap();
    let rpc_url = format!("https://eth-mainnet.g.alchemy.com/v2/{}", api_key);
    let provider = ProviderBuilder::new().on_http(Url::parse(rpc_url.as_str()).unwrap());

    let latest = BlockId::Number(BlockNumberOrTag::Latest);
    let latest_block = provider
        .get_block(latest, BlockTransactionsKind::Full)
        .await
        .unwrap()
        .unwrap();

    // lets create a partial state trie starting from the latest block's state root
    let mut state_trie = StateTrie::from_root(latest_block.header.state_root);

    // download EIP-1186 state proof for 0x0000000000000000000000000000000000000000.
    state_trie
        .load_proof(
            provider
                .get_proof(Address::ZERO, vec![B256::ZERO])
                .block_id(latest)
                .await
                .unwrap(),
        )
        .unwrap();

    println!("state root current: {:?}", state_trie.root());

    // yay eth burn!
    state_trie
        .account_trie
        .set_balance(Address::ZERO, U256::ZERO)
        .unwrap();

    println!("state root after burn: {:?}", state_trie.root());
}
