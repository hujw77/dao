use crate::bindings::time_lock::{
    CallScheduledFilter, TimeLock as TimeLockContract, TimeLockEvents,
};
use crate::cmd::utils::Bytes;
use async_recursion::async_recursion;
use ethers::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter, Result};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{convert::TryFrom, str::FromStr, sync::Arc};
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(about = "TimeLock related commands.")]
pub enum TimeLock {
    #[structopt(name = "min delay")]
    MinDelay,
    #[structopt(name = "proposal")]
    Proposals(Proposal),
    #[structopt(name = "role")]
    Roles(Role),
}

#[derive(StructOpt)]
#[structopt(about = "TimeLock proposal related commands.")]
pub enum Proposal {
    #[structopt(about = "Proposal list.")]
    List {
        #[structopt(default_value = "0x65c0c")]
        #[structopt(long, short)]
        from_block: U64,
        #[structopt(long, short)]
        to_block: Option<U64>,
        #[structopt(long)]
        no_done: bool,
        #[structopt(long)]
        no_ready: bool,
        #[structopt(long)]
        no_pending: bool,
        #[structopt(long)]
        no_cancel: bool,
    },
    #[structopt(about = "Schedule an proposal containing a single transaction.")]
    Schedule {
        #[structopt(
            about = "The address of the smart contract that the timelock should operate on."
        )]
        target: Address,
        #[structopt(
            about = "In wei, that should be sent with the transaction. Most of the time this will be 0."
        )]
        value: U256,
        #[structopt(
            about = "Containing the encoded function selector and parameters of the call by abi.encode."
        )]
        data: Bytes,
        #[structopt(about = "That specifies a dependency between operations.")]
        predecessor: H256,
        #[structopt(
            about = "Used to disambiguate two otherwise identical proposals. This can be any random value."
        )]
        salt: H256,
        #[structopt(about = "Delay time to execute the proposal, should be larger than minDelay")]
        delay: U256,
    },
    #[structopt(about = "Cancel an proposal.")]
    Cancel {
        #[structopt(about = "Proposal ID")]
        id: H256,
    },
    #[structopt(about = "Execute an (ready) proposal containing a single transaction.")]
    Execute {
        #[structopt(
            about = "The address of the smart contract that the timelock should operate on."
        )]
        target: Address,
        #[structopt(
            about = "In wei, that should be sent with the transaction. Most of the time this will be 0."
        )]
        value: U256,
        #[structopt(
            about = "Containing the encoded function selector and parameters of the call by abi.encode."
        )]
        data: Bytes,
        #[structopt(about = "That specifies a dependency between operations.")]
        predecessor: H256,
        #[structopt(
            about = "Used to disambiguate two otherwise identical proposals. This can be any random value."
        )]
        salt: H256,
    },
}

#[derive(StructOpt)]
#[structopt(about = "TimeLock role related commands.")]
pub enum Role {
    IsAdmin {
        account: Address,
    },
    IsProposer {
        account: Address,
    },
    IsExecutor {
        account: Address,
    },
    Grant {
        #[structopt(about = "1: admin, 2: proposer, 3: executor")]
        role: u8,
        account: Address,
    },
    Revoke {
        #[structopt(about = "1: admin, 2: proposer, 3: executor")]
        role: u8,
        account: Address,
    },
}

#[derive(Hash, Clone, Debug, Eq, PartialEq)]
pub enum ProposalStatus {
    Pending,
    Ready,
    Executed,
    Cancelled,
}

#[derive(Hash, Clone, Debug, Eq, PartialEq)]
pub struct ProposalItem {
    id: [u8; 32],
    index: U256,
    target: Address,
    value: U256,
    data: ethers::prelude::Bytes,
    predecessor: [u8; 32],
    delay: U256,
    status: ProposalStatus,
}

impl ProposalItem {
    fn from(filter: &CallScheduledFilter) -> Self {
        ProposalItem {
            id: filter.id,
            index: filter.index,
            target: filter.target,
            value: filter.value,
            data: filter.data.clone(),
            predecessor: filter.predecessor,
            delay: filter.delay,
            status: ProposalStatus::Pending,
        }
    }
}

impl Display for ProposalItem {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(
            f,
            "id: {}\nindex: {}\ntarget: {:?}\nvalue: {}\ndata: {}\npredecessor: {}\nstatus: {:?}",
            hex::encode(self.id),
            self.index,
            self.target,
            self.value,
            self.data,
            hex::encode(self.predecessor),
            self.status
        )
    }
}

impl TimeLock {
    pub async fn run(self) -> eyre::Result<()> {
        match self {
            TimeLock::MinDelay => {
                let time_lock = init_timelock_call().await?;
                let min_delay = time_lock.get_min_delay().call().await?;
                println!("{}", min_delay);
            }
            TimeLock::Proposals(_p) => _p.run().await?,
            TimeLock::Roles(_r) => _r.run().await?,
        }
        Ok(())
    }
}

impl Role {
    pub async fn run(self) -> eyre::Result<()> {
        match self {
            Role::IsAdmin { account } => {
                let time_lock = init_timelock_call().await?;
                let timelock_admin_role = time_lock.timelock_admin_role().call().await?;
                let is = time_lock
                    .has_role(timelock_admin_role, account)
                    .call()
                    .await?;
                println!("{}", is);
            }
            Role::IsProposer { account } => {
                let time_lock = init_timelock_call().await?;
                let proposer_role = time_lock.proposer_role().call().await?;
                let is = time_lock.has_role(proposer_role, account).call().await?;
                println!("{}", is);
            }
            Role::IsExecutor { account } => {
                let time_lock = init_timelock_call().await?;
                let executor_role = time_lock.executor_role().call().await?;
                let is = time_lock.has_role(executor_role, account).call().await?;
                println!("{}", is);
            }
            Role::Grant { role, account } => {
                let time_lock = init_timelock_call().await?;
                let role = if role == 1 {
                    time_lock.timelock_admin_role().call().await?
                } else if role == 2 {
                    time_lock.proposer_role().call().await?
                } else if role == 3 {
                    time_lock.executor_role().call().await?
                } else {
                    panic!("unexpect role");
                };
                let calldata = time_lock.grant_role(role, account).calldata().unwrap();
                println!("{}", calldata);
            }
            Role::Revoke { role, account } => {
                let time_lock = init_timelock_call().await?;
                let role = if role == 1 {
                    time_lock.timelock_admin_role().call().await?
                } else if role == 2 {
                    time_lock.proposer_role().call().await?
                } else if role == 3 {
                    time_lock.executor_role().call().await?
                } else {
                    panic!("unexpect role");
                };
                let calldata = time_lock.revoke_role(role, account).calldata().unwrap();
                println!("{}", calldata);
            }
        }
        Ok(())
    }
}

impl Proposal {
    pub async fn run(self) -> eyre::Result<()> {
        match self {
            Proposal::List {
                from_block,
                to_block,
                no_done,
                no_ready,
                no_pending,
                no_cancel,
            } => {
                load_proposals(
                    from_block, to_block, no_done, no_ready, no_pending, no_cancel,
                )
                .await?;
            }
            Proposal::Schedule {
                target,
                value,
                data,
                predecessor,
                salt,
                delay,
            } => {
                let time_lock = init_timelock_call().await?;
                let calldata = ethers::prelude::Bytes::from(data.0);
                let payload = time_lock
                    .schedule(
                        target,
                        value,
                        calldata,
                        *predecessor.as_fixed_bytes(),
                        *salt.as_fixed_bytes(),
                        delay,
                    )
                    .calldata()
                    .unwrap();
                println!("{}", payload);
            }
            Proposal::Cancel { id } => {
                let time_lock = init_timelock_call().await?;
                let calldata = time_lock.cancel(*id.as_fixed_bytes()).calldata().unwrap();
                println!("{}", calldata);
            }
            Proposal::Execute {
                target,
                value,
                data,
                predecessor,
                salt,
            } => {
                let time_lock = init_timelock_call().await?;
                let calldata = ethers::prelude::Bytes::from(data.0);
                let payload = time_lock
                    .execute(
                        target,
                        value,
                        calldata,
                        *predecessor.as_fixed_bytes(),
                        *salt.as_fixed_bytes(),
                    )
                    .calldata()
                    .unwrap();
                println!("{}", payload);
            }
        }
        Ok(())
    }
}

pub async fn load_proposals(
    from_block: U64,
    to_block: Option<U64>,
    no_done: bool,
    no_ready: bool,
    no_pending: bool,
    no_cancel: bool,
) -> eyre::Result<()> {
    let time_lock = init_timelock_call().await?;
    let _to_block = if let Some(to_block) = to_block {
        to_block
    } else {
        time_lock.client().get_block_number().await.unwrap()
    };
    let now = timestamp();
    let mut proposals: HashMap<[u8; 32], ProposalItem> = HashMap::new();
    let mut events = load_events(time_lock.clone(), &from_block, &_to_block).await;
    events.sort_by(|a, b| a.1.block_number.cmp(&b.1.block_number));
    for event in events {
        match &event.0 {
            TimeLockEvents::CallScheduledFilter(data) => {
                let mut proposal = ProposalItem::from(data);
                let ts = time_lock.get_timestamp(proposal.id).call().await?;
                if ts.as_u64() < now {
                    proposal.status = ProposalStatus::Ready;
                }
                proposals.insert(data.id, proposal);
            }
            TimeLockEvents::CallExecutedFilter(data) => {
                if proposals.contains_key(&data.id) {
                    proposals.get_mut(&data.id).unwrap().status = ProposalStatus::Executed;
                } else {
                    panic!("proposal not exist");
                }
            }
            TimeLockEvents::CancelledFilter(data) => {
                if proposals.contains_key(&data.id) {
                    proposals.get_mut(&data.id).unwrap().status = ProposalStatus::Cancelled;
                } else {
                    panic!("proposal not exist");
                }
            }
            TimeLockEvents::MinDelayChangeFilter(_) => {}
            TimeLockEvents::RoleAdminChangedFilter(_) => {}
            TimeLockEvents::RoleGrantedFilter(_) => {}
            TimeLockEvents::RoleRevokedFilter(_) => {}
        }
    }
    let mut statuses: HashSet<ProposalStatus> = [
        ProposalStatus::Executed,
        ProposalStatus::Pending,
        ProposalStatus::Ready,
        ProposalStatus::Cancelled,
    ]
    .iter()
    .cloned()
    .collect();
    if no_done {
        statuses.remove(&ProposalStatus::Executed);
    }
    if no_pending {
        statuses.remove(&ProposalStatus::Pending);
    }
    if no_ready {
        statuses.remove(&ProposalStatus::Ready);
    }
    if no_cancel {
        statuses.remove(&ProposalStatus::Cancelled);
    }
    proposals.retain(|_, v| -> bool { statuses.contains(&v.status) });
    for p in proposals.values() {
        println!("=============================================================================");
        println!("{}", p);
    }
    Ok(())
}

pub fn timestamp() -> u64 {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    since_the_epoch.as_secs()
}

#[async_recursion]
pub async fn load_events(
    contract: TimeLockContract<SignerMiddleware<Provider<Http>, Wallet<k256::ecdsa::SigningKey>>>,
    from_block: &U64,
    to_block: &U64,
) -> Vec<(TimeLockEvents, LogMeta)> {
    let events = contract
        .events()
        .from_block(from_block)
        .to_block(to_block)
        .query_with_meta()
        .await;

    match events {
        Ok(result) => result,
        Err(err) => {
            println!("Query err: {:?}", err);

            let mid_block = (from_block + to_block) / 2u64;
            if mid_block == *from_block || mid_block == *to_block {
                panic!("range is already narrow");
            }

            let mut left_part = load_events(contract.clone(), from_block, &mid_block).await;

            let mut right_part = load_events(contract.clone(), &(mid_block + 1u64), to_block).await;
            left_part.append(&mut right_part);
            left_part
        }
    }
}

pub async fn init_timelock_call(
) -> eyre::Result<TimeLockContract<SignerMiddleware<Provider<Http>, Wallet<k256::ecdsa::SigningKey>>>>
{
    Ok(init_timelock_send(
        "380eb0f3d505f087e438eca80bc4df9a7faa24f868e69fc0440261a0fc0567dc".to_string(),
    )
    .await?)
}

pub async fn init_timelock_send(
    private_key: String,
) -> eyre::Result<TimeLockContract<SignerMiddleware<Provider<Http>, Wallet<k256::ecdsa::SigningKey>>>>
{
    // let provider = Provider::<Http>::try_from("https://crab-rpc.darwinia.network")?;
    let provider = Provider::<Http>::try_from("https://pangolin-rpc.darwinia.network")?;
    let chain_id = provider.get_chainid().await.unwrap().as_u64();
    let key = private_key
        .parse::<LocalWallet>()
        .unwrap()
        .with_chain_id(chain_id);
    let to = Address::from_str("0x4214611Be6cA4E337b37e192abF076F715Af4CaE")?;
    // pangolin
    // let to = Address::from_str("0x2401224012bAE7C2f217392665CA7abC16dCDE1e")?;
    // crab
    // let to = Address::from_str("0xED1d1d219f85Bc634f250db5e77E0330Cddc9b2a")?;
    let client = SignerMiddleware::new(provider, key);
    let client = Arc::new(client);
    let time_lock = TimeLockContract::new(to, client);
    Ok(time_lock)
}