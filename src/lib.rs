#![forbid(unsafe_code)]

mod config;
mod lang;
mod mining;
mod queue;
mod walk;

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use strawberry_plugin_api::uuid::Uuid;
use strawberry_plugin_api::{
    Context, ItemStack, Player, Plugin, PluginMetadata, Result, Server, World,
    command::{Command, CommandError, CommandSender, ConsumedArgs},
    commands::CommandHandler,
    events::{
        BlockBreakEvent, EventData, EventHandler, FailureVerdict, HandlerOptions, HandlerPriority,
        PlayerLeaveEvent,
    },
    permission::{Permission, PermissionDefault},
    register_plugin,
    scheduler::{self, SchedulerExt},
    text::TextComponent,
    wit::strawberry::plugin::{
        common::{BlockPos, GameMode, Hand},
        event::BlockBreakEventData,
    },
};

use config::Config;
use mining::ToolIdentity;
use queue::{Owner, Queue, Ticket};
use walk::{Budget, Host, Pos, Walk};

const USE: &str = "fastminer:use";
const FILE: &str = "data/config.txt";

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}
fn owner(p: &Player) -> Owner {
    let u = p.get_id();
    (u.high, u.low)
}
fn pos(p: Pos) -> BlockPos {
    BlockPos {
        x: p[0],
        y: p[1],
        z: p[2],
    }
}
fn identity(tool: &ItemStack) -> ToolIdentity {
    ToolIdentity::new(tool.get_registry_key(), tool.get_components())
}

struct Definition {
    kind: mining::Kind,
    states: Vec<u16>,
}
struct Job {
    root: Pos,
    world: String,
    tool: ToolIdentity,
    slot: u8,
    definition: Arc<Definition>,
    walk: Walk,
    confirmed: bool,
}
struct State {
    config: Config,
    registry: HashMap<String, Arc<Definition>>,
    air: Vec<u16>,
    queue: Mutex<Queue<Job>>,
    task: Mutex<Option<u32>>,
}
struct Fastminer {
    state: Mutex<Option<Arc<State>>>,
}

fn config_load() -> std::result::Result<Config, String> {
    let mut file = match std::fs::File::open(FILE) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(FILE)
                .map_err(|_| lang::text("en", "config_io", &[]))?;
            f.write_all(config::DEFAULT.as_bytes())
                .map_err(|_| lang::text("en", "config_io", &[]))?;
            std::fs::File::open(FILE).map_err(|_| lang::text("en", "config_io", &[]))?
        }
        Err(_) => return Err(lang::text("en", "config_io", &[])),
    };
    let mut text = String::new();
    (&mut file)
        .take(4097)
        .read_to_string(&mut text)
        .map_err(|_| lang::text("en", "config_io", &[]))?;
    Config::parse(&text).map_err(|line| lang::text("en", "config_error", &[&line.to_string()]))
}

impl Plugin for Fastminer {
    fn new() -> Self {
        Self {
            state: Mutex::new(None),
        }
    }
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "fastminer".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["Strawberry Interactive LLC".into()],
            description: lang::text("en", "description", &[]),
            dependencies: vec![],
            permissions: vec!["fs.read.data".into(), "fs.write.data".into()],
        }
    }
    fn on_load(&self, context: Context) -> Result<()> {
        let config = config_load()?;
        let mut registry = HashMap::new();
        for name in mining::blocks() {
            if let Some(block) =
                strawberry_plugin_api::world::get_block_by_name(&format!("minecraft:{name}"))
            {
                registry.insert(
                    name.clone(),
                    Arc::new(Definition {
                        kind: mining::kind(&name)
                            .ok_or_else(|| lang::text("en", "registry_error", &[]))?,
                        states: block.state_ids,
                    }),
                );
            }
        }
        if registry.is_empty() {
            return Err(lang::text("en", "registry_error", &[]));
        }
        let air = strawberry_plugin_api::world::get_block_by_name("minecraft:air")
            .ok_or_else(|| lang::text("en", "registry_error", &[]))?
            .state_ids;
        let state = Arc::new(State {
            config,
            registry,
            air,
            queue: Mutex::new(Queue::default()),
            task: Mutex::new(None),
        });
        context.register_permission(&Permission {
            node: USE.into(),
            description: lang::text("en", "permission", &[]),
            default: PermissionDefault::Allow,
            children: vec![],
        })?;
        context.register_command(
            Command::new(&["fastminer".into()], &lang::text("en", "command", &[]))
                .execute(Status(Arc::clone(&state))),
            USE,
        );
        context.register_event_handler_with_options::<BlockBreakEvent, _>(
            Start(Arc::clone(&state)),
            HandlerOptions {
                priority: HandlerPriority::Monitor,
                blocking: true,
                ignore_cancelled: true,
                on_failure: FailureVerdict::Allow,
            },
        )?;
        context.register_event_handler_with_options::<PlayerLeaveEvent, _>(
            Leave(Arc::clone(&state)),
            HandlerOptions {
                priority: HandlerPriority::Monitor,
                blocking: true,
                ignore_cancelled: false,
                on_failure: FailureVerdict::Allow,
            },
        )?;
        *lock(&self.state) = Some(state);
        Ok(())
    }
    fn on_unload(&self, _context: Context) -> Result<()> {
        let state = lock(&self.state).take();
        if let Some(state) = state {
            lock(&state.queue).close();
            let task = lock(&state.task).take();
            if let Some(task) = task {
                scheduler::cancel_task(task);
            }
        }
        Ok(())
    }
}

struct Status(Arc<State>);
impl CommandHandler for Status {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let locale = format!("{:?}", sender.get_locale());
        sender.send_message(TextComponent::text(&lang::text(&locale, "usage", &[])));
        sender.send_message(TextComponent::text(&lang::text(
            &locale,
            "status",
            &[
                &self.0.config.max_blocks.to_string(),
                &self.0.config.enabled.to_string(),
            ],
        )));
        Ok(1)
    }
}

struct Leave(Arc<State>);
impl EventHandler<PlayerLeaveEvent> for Leave {
    fn handle(
        &self,
        _server: Server,
        event: EventData<PlayerLeaveEvent>,
    ) -> EventData<PlayerLeaveEvent> {
        let id = owner(&event.player);
        lock(&self.0.queue).cancel(id);
        event
    }
}

struct Start(Arc<State>);
impl EventHandler<BlockBreakEvent> for Start {
    fn handle(&self, server: Server, event: BlockBreakEventData) -> BlockBreakEventData {
        let state = &self.0;
        if event.cancelled || !state.config.enabled || state.config.max_blocks < 2 {
            return event;
        }
        let Some(player) = &event.player else {
            return event;
        };
        let id = owner(player);
        // Any in-flight synthetic break is observed, never used as a new seed.
        {
            let q = lock(&state.queue);
            if q.closed
                || q.synthetic.is_some()
                || q.active.contains_key(&id)
                || q.active.len() >= state.config.max_jobs
            {
                return event;
            }
        }
        let name = event
            .block
            .strip_prefix("minecraft:")
            .unwrap_or(&event.block);
        let Some(definition) = state.registry.get(name) else {
            return event;
        };
        if match definition.kind {
            mining::Kind::Log => !state.config.logs,
            mining::Kind::Ore(_) => !state.config.ores,
        } {
            return event;
        }
        if !player.as_entity().is_sneaking()
            || !player.has_permission(USE)
            || !matches!(
                player.get_gamemode(),
                GameMode::Survival | GameMode::Creative
            )
            || player.get_health() <= 0.0
        {
            return event;
        }
        let inventory = player.get_inventory();
        let slot = inventory.get_selected_slot();
        let Some(tool) = inventory.get_item_in_hand(Hand::Right) else {
            return event;
        };
        if !mining::correct_tool(definition.kind, &tool.get_registry_key())
            || state
                .config
                .excluded_tool_data
                .iter()
                .any(|(namespace, key)| tool.has_custom_data(namespace, key))
        {
            return event;
        }
        let root = [event.block_pos.x, event.block_pos.y, event.block_pos.z];
        let Some(world) = strawberry_plugin_api::events_wit::event_world() else {
            return event;
        };
        let world = world.get_id();
        if !state.config.allows_world(&world) {
            return event;
        }
        let job = Job {
            root,
            world,
            tool: identity(&tool),
            slot,
            definition: Arc::clone(definition),
            walk: Walk::new(root, state.config.max_blocks),
            confirmed: false,
        };
        if lock(&state.queue)
            .insert(id, job, state.config.max_jobs)
            .is_some()
        {
            schedule(state, &server);
        }
        event
    }
}

fn schedule(state: &Arc<State>, server: &Server) {
    {
        let mut q = lock(&state.queue);
        if q.closed || q.scheduled || q.items.is_empty() {
            return;
        }
        q.scheduled = true;
    }
    let captured = Arc::clone(state);
    let task = server.schedule_delayed_task(1, move |server| tick(&captured, &server));
    *lock(&state.task) = Some(task);
}

fn tick(state: &Arc<State>, server: &Server) {
    *lock(&state.task) = None;
    // Retain scheduled=true during callbacks so reentrant events cannot create a second task.
    let mut budget = Budget {
        probes: state.config.probes_per_tick,
        breaks: state.config.breaks_per_tick,
    };
    let mut turns = 0;
    while budget.probes > 0 && budget.breaks > 0 && turns < state.config.probes_per_tick {
        turns += 1;
        let Some((ticket, mut job)) = lock(&state.queue).items.pop_front() else {
            break;
        };
        if !lock(&state.queue).live(ticket) {
            continue;
        }
        let player = server.get_player_by_uuid(Uuid {
            high: ticket.0.0,
            low: ticket.0.1,
        });
        let keep = if let Some(player) = player {
            let world = player.get_world();
            let valid = Adapter {
                state,
                ticket,
                player: &player,
                world: &world,
                expected_world: &job.world,
                tool: &job.tool,
                slot: job.slot,
                definition: &job.definition,
            }
            .valid();
            if !valid {
                false
            } else if !job.confirmed {
                // A Monitor event is still pre-commit. Never chain a cancelled/failed seed.
                budget.probes -= 1;
                if read_state(&world, job.root).is_some_and(|id| state.air.contains(&id)) {
                    job.confirmed = true;
                    true
                } else {
                    false
                }
            } else {
                // Borrow disjoint job fields; no temporary walker or state lock over host calls.
                let mut host = Adapter {
                    state,
                    ticket,
                    player: &player,
                    world: &world,
                    expected_world: &job.world,
                    tool: &job.tool,
                    slot: job.slot,
                    definition: &job.definition,
                };
                let breaks = budget.breaks;
                for _ in 0..26 {
                    job.walk.advance(&mut host, &mut budget);
                    if job.walk.done() || budget.probes == 0 || budget.breaks < breaks {
                        break;
                    }
                }
                !job.walk.done()
            }
        } else {
            false
        };
        let mut q = lock(&state.queue);
        if keep && q.live(ticket) {
            q.items.push_back((ticket, job));
        } else {
            q.finish(ticket);
        }
    }
    lock(&state.queue).scheduled = false;
    schedule(state, server);
}

fn read_state(world: &World, p: Pos) -> Option<u16> {
    if p[0].unsigned_abs() > 29_999_999
        || p[2].unsigned_abs() > 29_999_999
        || !(-2048..=2047).contains(&p[1])
    {
        return None;
    }
    let chunk = world.get_chunk(p[0].div_euclid(16), p[2].div_euclid(16))?;
    Some(chunk.get_block_state_id(pos([p[0].rem_euclid(16), p[1], p[2].rem_euclid(16)])))
}

struct Adapter<'a> {
    state: &'a State,
    ticket: Ticket,
    player: &'a Player,
    world: &'a World,
    expected_world: &'a str,
    tool: &'a ToolIdentity,
    slot: u8,
    definition: &'a Definition,
}
impl Adapter<'_> {
    fn valid(&self) -> bool {
        if !lock(&self.state.queue).live(self.ticket) {
            return false;
        }
        let p = self.player;
        p.get_world().get_id() == self.expected_world
            && p.as_entity().is_sneaking()
            && p.get_health() > 0.0
            && p.has_permission(USE)
            && matches!(p.get_gamemode(), GameMode::Survival | GameMode::Creative)
            && p.get_inventory().get_selected_slot() == self.slot
            && p.get_inventory()
                .get_item_in_hand(Hand::Right)
                .is_some_and(|tool| tool.get_count() > 0 && identity(&tool) == *self.tool)
    }
}
impl Host for Adapter<'_> {
    fn same_kind(&mut self, p: Pos) -> bool {
        read_state(self.world, p).is_some_and(|id| self.definition.states.contains(&id))
            && self
                .world
                .get_border()
                .contains(f64::from(p[0]) + 0.5, f64::from(p[2]) + 0.5)
    }
    fn break_one(&mut self, p: Pos) -> bool {
        if !self.valid() {
            return false;
        }
        lock(&self.state.queue).synthetic = Some((self.ticket.0, p));
        let broken = self.player.break_block(pos(p));
        lock(&self.state.queue).synthetic = None;
        broken
    }
}

register_plugin!(Fastminer);
