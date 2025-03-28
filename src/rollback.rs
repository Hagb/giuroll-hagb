#[cfg(feature = "logtofile")]
use log::info;
use std::{
    arch::asm,
    collections::{HashMap, HashSet},
    ffi::c_void,
    iter::Empty,
    ops::Deref,
    ptr::null_mut,
    time::Duration,
};

use windows::Win32::Foundation::HANDLE;

#[allow(unused_imports)]
use crate::println;
use crate::{
    ptr_wrap, set_input_buffer, soku_heap_free, Callbacks, CameraTransform, CALLBACK_ARRAY,
    INPUT_KEYS_NUMBERS, ISDEBUG, LAST_CAMERA_BEFORE_SMOOTH, MEMORY_RECEIVER_ALLOC,
    MEMORY_RECEIVER_FREE, SOKU_FRAMECOUNT, SOUND_MANAGER,
};

type RInput = [bool; INPUT_KEYS_NUMBERS];

pub static mut CHARSIZEDATA: Vec<(usize, usize)> = vec![];

#[no_mangle]
pub unsafe extern "cdecl" fn set_char_data_size(s: usize) {
    while CHARSIZEDATA.len() < s {
        CHARSIZEDATA.push((0, 0))
    }
}

#[no_mangle]
pub unsafe extern "cdecl" fn set_char_data_pos(pos: usize, a: usize, b: usize) {
    set_char_data_size(pos);
    CHARSIZEDATA[pos] = (a, b);
}

// pub enum MemoryManip {
//     Alloc(usize),
//     Free(usize),
// }
pub struct EnemyInputHolder {
    pub i: Vec<Option<RInput>>,
}

impl EnemyInputHolder {
    fn new() -> Self {
        Self { i: Vec::new() }
    }
    fn get(&self, count: usize) -> RInput {
        match self.get_result(count) {
            Ok(x) => x,
            Err(x) => x,
        }
    }

    pub fn insert(&mut self, input: RInput, frame: usize) {
        while frame >= self.i.len() {
            self.i.push(None);
        }
        if let Some(x) = self.i[frame].replace(input) {
            //doubled input
            if x != input {
                panic!("replacing existing input");
            }
        }
    }

    fn get_result(&self, frame: usize) -> Result<RInput, RInput> {
        match self.i.get(frame) {
            Some(Some(x)) => Ok(*x),
            None if frame == 0 => Err([false; INPUT_KEYS_NUMBERS]),
            Some(None) | None => {
                /*
                    in the future maybe try dropping inputs for attacks that are about to charge?
                    let mut w = (1..3)
                        .map(|x| self.get(frame.saturating_sub(x)))
                        .reduce(|x, y| {
                            (0..INPUT_KEYS_NUMBERS)
                                .map(|idx| x[idx] & y[idx])
                                .collect::<Vec<_>>()
                                .try_into()
                                .unwrap()
                        })
                        .unwrap();

                    w[0..4].copy_from_slice(&self.get(frame - 1)[0..4]);
                */

                Err(self.get(frame - 1))
            }
        }
    }
}

pub struct Rollbacker {
    pub guessed: Vec<RollFrame>,

    current: usize,
    rolling_back: bool,

    // pub future_sound: HashMap<usize, usize>,
    // first element is the sound, second is the frame it occured at, whenever a frame comes true we can delete all future sounds with that value

    // stores all the sounds that happened in "guessed" frames. Will also need to be topped up *after* last frame.
    // every frame we can store this as "past_sounds", and if any sound in future sounds did not appear in past_sounds, we can then cancell that sound by force calling 0x401d50
    // which we hook, and set a static to ignore the sound.

    //also, while rolling back, we should not play sounds that already did appear in past_sounds (and instead remove them, so we can see what is)
    pub enemy_inputs: EnemyInputHolder,
    pub self_inputs: Vec<RInput>,

    pub weathers: HashMap<usize, u8>,
}

impl Rollbacker {
    pub fn new() -> Self {
        Self {
            guessed: Vec::new(),
            current: 0,
            rolling_back: false,
            enemy_inputs: EnemyInputHolder::new(),
            self_inputs: Vec::new(),
            weathers: HashMap::new(),
            // future_sound: HashMap::new(),
        }
    }

    /// fill in inputs before calling this function
    pub fn start(&mut self) -> usize {
        //this should only be called on the 0th iteration.
        self.current = unsafe { *SOKU_FRAMECOUNT };
        //let newsound = std::mem::replace(&mut *SOUNDS_THAT_DID_HAPPEN.lock().unwrap(), BTreeMap::new());

        while self.guessed.len() > 0
            && (self
                .enemy_inputs
                .get_result(self.guessed[0].prev_state.number)
                .map(|x| x == self.guessed[0].enemy_input)
                .unwrap_or(false))
        {
            let mut m = self.guessed.remove(0);

            self.weathers
                .insert(m.prev_state.number, m.prev_state.weather_sync_check);
            m.prev_state.did_happen();
            #[cfg(feature = "logrollback")]
            println!("did_happen {}", m.prev_state.number);
            //let b = &mut *FREEMUTEX.lock().unwrap();
            //for a in m.prev_state.frees {
            //    b.insert(a);
            //}
        }

        //*SOUND_DELET_MUTEX.lock().unwrap() = newsound;

        self.rolling_back = false;
        self.guessed.len() + 1
    }

    fn apply_input(input: RInput, opponent_input: RInput) {
        #[cfg(feature = "logrollback")]
        println!("apply input {:?}", opponent_input);
        let is_p1 = unsafe {
            let netmanager = *(0x8986a0 as *const usize);
            *ptr_wrap!(netmanager as *const usize) == 0x858cac
        };

        if is_p1 {
            unsafe { set_input_buffer(input, opponent_input) };
        } else {
            unsafe { set_input_buffer(opponent_input, input) };
        }
    }

    pub fn step(&mut self, iteration_number: usize) -> Option<()> {
        let tbr = if self.guessed.len() == iteration_number {
            //last iteration for this frame, handle sound here
            if self.rolling_back {
                unsafe {
                    let manager = SOUND_MANAGER.as_mut().unwrap();
                    manager.delete_non_matched();
                }
            }
            /*
            let mut to_be_skipped = vec![];
            {
                let new_sounds = &mut *SOUNDS_THAT_DID_HAPPEN.lock().unwrap();
                let old_sounds = &mut *SOUND_THAT_MAYBE_HAPPEN.lock().unwrap();

                let new_col = new_sounds
                    .values()
                    .map(|x| x.into_iter())
                    .flatten()
                    .collect::<HashSet<_>>();

                for idx in (self.current).saturating_sub(INPUT_KEYS_NUMBERS)..=(self.current + 1) {
                    if new_sounds.contains_key(&(self.current + 1)) {
                        println!("HERE, CONTAINS")
                    }
                    if !new_sounds.contains_key(&idx) {
                        continue;
                    }
                    if let Some(x) = old_sounds.get(&idx) {
                        for a in x {
                            if !new_col.contains(a) {
                                to_be_skipped.push(*a);
                            }
                        }
                    }
                }

                std::mem::swap(old_sounds, new_sounds);
            };
            if to_be_skipped.len() != 0{
                println!("len: {}", to_be_skipped.len());
            }
            for a in to_be_skipped {
                force_sound_skip(a);
            }
            */

            //if self.current != unsafe { *SOKU_FRAMECOUNT } {
            //    println!("here");
            //}

            let current = unsafe { *SOKU_FRAMECOUNT };

            let si = self.self_inputs[current];
            let ei = self.enemy_inputs.get(current);
            Self::apply_input(si, ei);
            self.guessed.push(RollFrame::dump_with_guess(si, ei));

            Some(())
        } else {
            let (fr_, remain) = self.guessed[iteration_number..].split_at_mut(1);
            let fr = &mut fr_[0];
            if self.rolling_back {
                unsafe {
                    fr.prev_state = dump_frame(None::<Empty<_>>, None::<Empty<_>>);
                    #[cfg(feature = "logrollback")]
                    println!("dump {}", fr.prev_state.number);
                    // the following have been done when `restore`:
                    // let prev = std::mem::replace(&mut fr.prev_state, frame);
                    // prev.never_happened();
                    //let b = &mut *ALLOCMUTEX.lock().unwrap();
                    //for a in prev.allocs {
                    //    b.insert(a);
                    //}
                };
                fr.enemy_input = self.enemy_inputs.get(fr.prev_state.number);
                Self::apply_input(fr.player_input, fr.enemy_input);
                Some(())
            } else if fr.enemy_input != self.enemy_inputs.get(fr.prev_state.number) {
                //info!("ROLLBACK");
                unsafe {
                    let manager = SOUND_MANAGER.as_mut().unwrap();
                    manager.pop_sounds_since(fr.prev_state.number, self.current);
                }
                self.rolling_back = true;
                fr.prev_state.restore(
                    Some(remain.iter_mut().map(|x| &mut x.prev_state)),
                    None::<Empty<_>>,
                    None::<Empty<_>>,
                );
                #[cfg(feature = "logrollback")]
                println!("restore {}", fr.prev_state.number);
                //fr.prev_state.clone().never_happened();

                fr.enemy_input = self.enemy_inputs.get(fr.prev_state.number);
                Self::apply_input(fr.player_input, fr.enemy_input);
                Some(())
            } else {
                None
            }
        };

        tbr
    }
}

pub struct RollFrame {
    pub prev_state: Frame,
    pub player_input: RInput,
    pub enemy_input: RInput,
}

pub static mut LAST_M_LEN: usize = 0;

impl RollFrame {
    fn dump_with_guess(player_input: RInput, guess: RInput) -> Self {
        let prev_state = unsafe { dump_frame(None::<Empty<_>>, None::<Empty<_>>) };
        #[cfg(feature = "logrollback")]
        println!("dump {} with guess", prev_state.number);

        Self {
            prev_state,
            player_input: player_input,
            enemy_input: guess,
        }
    }
}
static mut FPST: [u8; 108] = [0u8; 108];
pub static mut DUMP_FRAME_TIME: Option<Duration> = None;
pub static mut MEMORY_LEAK: usize = 0;
pub unsafe fn dump_frame(
    extra_allocs: Option<impl Iterator<Item = usize>>,
    extra_frees: Option<impl Iterator<Item = usize>>,
) -> Frame {
    let w = unsafe {
        //let b = 3;
        asm!(
            "FSAVE {fpst}",
            "FRSTOR {fpst}",
            fpst = sym FPST
        );
        FPST
    };
    use std::time::Instant;
    let now: Option<Instant> = match DUMP_FRAME_TIME {
        None => None,
        Some(_) => Some(Instant::now()),
    };
    // println!("dump {}", *SOKU_FRAMECOUNT);

    // guess the length to avoid reallocation as far as possible
    let mut m: Vec<ReadAddr> = Vec::with_capacity(LAST_M_LEN.next_power_of_two());

    #[cfg(feature = "logtofile")]
    if ISDEBUG {
        info!("0x895ec")
    };
    let ptr1 = read_addr(0x8985ec, 0x4);
    let first = get_ptr(&ptr1.content[0..4], 0);
    m.push(read_addr(first, 0xec));

    {
        let t = read_vec(first + 0x1c);

        m.push(t.read_underlying());

        m.push(t.to_addr());
    }

    {
        let t = read_vec(first + 0x68);
        if t.start != 0 {
            m.push(t.read_underlying());
        }
    }

    {
        let t = read_linked_list(first + 0x78);

        m.extend(t.read_all(0));
    }

    {
        let t = read_linked_list(first + 0xa4);

        m.extend(t.read_all(0x180));
    }

    {
        m.extend(read_maybe_ring_buffer(first + 0x28).read_whole(0x10));
    }
    #[cfg(feature = "logtofile")]
    //0x8985e0
    if ISDEBUG {
        info!("0x8985e0")
    };
    let ptr1 = read_addr(0x8985e0, 0x4);
    let first = get_ptr(&ptr1.content[0..4], 0);
    m.push(read_addr(first, 0x118));

    m.extend(read_linked_list(first + 0x4).read_all(0));

    let llautosize = read_linked_list(first + 0x2c);
    m.push(read_addr(first + 0x2c + 0xc, 4));

    let mut lit = llautosize.read_underlying();
    m.push(llautosize.to_addr());
    m.push(lit.next().unwrap().to_addr());

    for a in lit {
        let p = a.additional_data;
        if p != 0 {
            let size = match read_heap(p) {
                0 => 0x70, //very weird, but this is what sokuroll does
                x => x,
            };

            m.push(read_addr(p, size));
            m.push(a.to_addr());
        }
    }

    m.extend(read_linked_list(first + 0x38).read_all(0));

    #[cfg(feature = "logtofile")]
    //0x8985f0
    if ISDEBUG {
        info!("0x8985f0")
    };
    //let ptr1 = read_addr(0x8985f0, 0x4);
    //let first = get_ptr(&ptr1.content[0..4], 0);

    let first = *(0x8985f0 as *const usize);

    m.push(read_addr(first, 0x94));

    m.push(read_vec(first + 0x10).read_underlying());

    m.push(read_vec(first + 0x20).read_underlying());
    #[cfg(feature = "logtofile")]
    if ISDEBUG {
        info!("0x8985f02")
    };
    m.extend(read_linked_list(first + 0x30).read_all(0));

    #[cfg(feature = "logtofile")]
    if ISDEBUG {
        info!("0x8985f03")
    };

    // effect_linked_list
    m.extend(read_linked_list(first + 0x5c).read_all(0x178));

    #[cfg(feature = "logtofile")]
    //0x8985e8
    if ISDEBUG {
        info!("0x8985e8")
    };
    unsafe fn read_weird_structure(m: &mut Vec<ReadAddr>, pos: usize, size: usize) {
        //I'm not quite sure what's going on here, or if it's infact correct
        let dat = read_addr(pos, 0x14);
        let n = dat.usize_align();

        let v1 = n[2];
        let v2 = n[3];
        let read_from = n[1];
        let v3 = n[4];

        if read_from == 0 {
            //println!("read_from is zero {:?}", n);
            if n[2] != 0 || n[3] != 0 || n[4] != 0 {
                #[cfg(feature = "logtofile")]
                if ISDEBUG {
                    info!("read_from is zero {:?}", n)
                };
            }
        } else {
            m.push(read_addr(read_from, v1 * 4));
        }
        for a in 0..v3 {
            let addr = *ptr_wrap!((read_from + ((a + v2) % v1) * 4) as *const usize);

            m.push(read_addr(addr, size));
        }
    }

    let ptr1 = read_addr(0x8985e8, 0x4);
    let first = get_ptr(&ptr1.content[0..4], 0);

    m.push(read_addr(first, 0x688));

    m.push(read_vec(first + 0x14).read_underlying());
    m.push(read_vec(first + 0x24).read_underlying());

    m.extend(read_linked_list(first + 0x34).read_all(0));

    m.extend(read_linked_list(first + 0x60).read_all(0x178));

    read_weird_structure(&mut m, first + 0x18c, 0xc);
    read_weird_structure(&mut m, first + 0x1c0, 0xc);

    #[cfg(feature = "logtofile")]
    //0x8985e4
    if ISDEBUG {
        info!("0x8985e4")
    };

    let p_battle_manager = read_addr(0x8985e4, 0x4);
    let p_battle_manager = get_ptr(&p_battle_manager.content[0..4], 0);
    m.push(read_addr(p_battle_manager, 0x908));
    m.extend(read_linked_list(p_battle_manager + 0x30).read_all(0));
    m.extend(read_linked_list(p_battle_manager + 0x3c).read_all(0));
    m.extend(read_linked_list(p_battle_manager + 0x48).read_all(0));
    m.extend(read_linked_list(p_battle_manager + 0x54).read_all(0));
    m.extend(read_linked_list(p_battle_manager + 0x60).read_all(0));
    m.extend(read_linked_list(p_battle_manager + 0x6c).read_all(0));

    {
        let w = read_vec(p_battle_manager + 0x9c);
        if w.start != 0 {
            m.push(w.read_underlying());
            #[cfg(feature = "logtofile")]
            info!("battle+x9c wasn't 0");
        }
        let w = read_vec(p_battle_manager + 0xac);

        if w.start != 0 {
            m.push(w.read_underlying());
            #[cfg(feature = "logtofile")]
            //seems to have never triggered, same as the one above
            info!("battle+xac wasn't 0");
        }
    }
    m.extend(read_linked_list(p_battle_manager + 0xbc).read_all(0));

    m.extend(read_linked_list(p_battle_manager + 0xe8).read_all(0));

    //0x8985dc
    if ISDEBUG {
        #[cfg(feature = "logtofile")]
        info!("0x8985dc")
    };

    //0x8986a0
    #[cfg(feature = "logtofile")]
    if ISDEBUG {
        info!("0x8986a0")
    };

    //here sokuroll locks a mutex, but it seems unnecceseary

    let ptr1 = read_addr(0x8986a0, 0x4);
    let first = get_ptr(&ptr1.content[0..4], 0);
    // netplay input buffer. TODO: find corresponding input buffers in replay mode
    if first != 0 {
        m.push(read_addr(first + 0xf8, 0x68));
        m.push(read_addr(first + 0x174, 0x68));
    }

    #[cfg(feature = "logtofile")]
    if ISDEBUG {
        info!("0x8985e4")
    };

    unsafe fn read_player_data(player: usize, m: &mut Vec<ReadAddr>) {
        let read_bullets = |pos: usize, char: u8, m: &mut Vec<_>| {
            let list = read_linked_list(pos);

            m.extend(list.read_all(0));

            let und = list.read_underlying();

            for a in und.skip(1) {
                m.push(a.to_addr());
                let d = a.additional_data;
                if d != 0 {
                    let z = CHARSIZEDATA[char as usize].1;
                    let bullet = read_addr(d, z);
                    let p1 = get_ptr(&bullet.content, 0x3a4);

                    if p1 != 0 {
                        let ll = read_linked_list(d + 0x3a4);

                        m.extend(ll.read_all(0));
                    }

                    let p1 = get_ptr(&bullet.content, 0x17c);
                    if p1 != 0 {
                        let ll = read_linked_list(d + 0x17c);
                        m.extend(ll.read_all(0));
                    }

                    let p3 = get_ptr(&bullet.content, 0x35c);
                    if p3 != 0 {
                        let s = read_heap(p3);
                        if s > 4000 {
                            panic!("bullet data too big! {}", s)
                        } else {
                            m.push(read_addr(p3, s));
                        }
                    }

                    let p4 = get_ptr(&bullet.content, 0x354);
                    m.push(bullet);
                    if p4 != 0 {
                        let nd = read_addr(p4, 0x54);

                        let size = usize::from_le_bytes(nd.content[0x30..0x34].try_into().unwrap());
                        let ptr = usize::from_le_bytes(nd.content[0x2c..0x30].try_into().unwrap());

                        let n2 = read_addr(ptr, size * 4);

                        for a in 0..size {
                            let p = get_ptr(&n2.content, a * 4);
                            if p != 0 {
                                m.push(read_addr(p, 0x10));
                            }
                        }

                        let size = usize::from_le_bytes(nd.content[0x44..0x48].try_into().unwrap());
                        let ptr = usize::from_le_bytes(nd.content[0x40..0x44].try_into().unwrap());
                        m.push(n2);

                        let n2 = read_addr(ptr, size * 4);

                        for a in 0..size {
                            let p = get_ptr(&n2.content, a * 4);
                            if p != 0 {
                                m.push(read_addr(p, 0x10));
                            }
                        }

                        let size = usize::from_le_bytes(nd.content[0x8..0xc].try_into().unwrap())
                            * usize::from_le_bytes(nd.content[0x4..0x8].try_into().unwrap())
                            * 2
                            + 2;

                        let ptr = usize::from_le_bytes(nd.content[0x50..0x54].try_into().unwrap());
                        m.push(n2);
                        m.push(nd);

                        m.push(read_addr(ptr, size));
                    }
                }
            }
        };

        let char = player + 0x34c;
        let char = *(char as *const u8);

        assert_ne!(
            CHARSIZEDATA[char as usize].0, 0,
            "The data size of character {} is missing?",
            char
        );
        let cdat = read_addr(player, CHARSIZEDATA[char as usize].0);

        let bullets = player + 0x17c;
        read_bullets(bullets, char, m);

        if char == 5 {
            //youmu
            read_weird_structure(m, player + 0x8bc, 0x2c);
        }

        if char == 36 {
            // Mamizou of CharacterEngine (https://github.com/SokuDev/CharacterEngine).
            // Hardcoding it is just a temporary workaround.
            // TODO: it should be exposed with APIs and implemented by developers of characters mods.
            // More specific:
            // - Comments. renaming, and even refactoring (for self-documenting code) to figure what they are exactly (mostly for myself to write APIs clearly);
            // - APIs for developers to ask GR to save/restore an address as a specific data structure, mostly Player, List, Vector, GameObject, String, and sized array;
            // - C++ header for these APIs.
            let extra_char = get_ptr(&cdat.content, 0x890);
            if extra_char != 0 {
                read_player_data(extra_char, m);
            }
        }

        let ll = read_linked_list(player + 0x718);

        m.push(read_addr(ll.ll4, 0xf4));

        for _ in 0..ll.listcount {
            let zcop = m.last().unwrap();
            let ptr = get_ptr(&zcop.content, 0);
            m.push(read_addr(ptr, 0xf4));
        }

        let new = get_ptr(&cdat.content, 0x6f8);
        m.push(cdat);

        m.push(read_addr(new, 0x68));

        let p4 = read_vec(new + 0x10);
        let w = p4.read_underlying();

        let i = p4.maybecapacity - p4.start;
        let i = (((i >> 0x1f) & 3) + i) >> 2;

        for a in 0..i {
            let p = get_ptr(&w.content, a * 4);

            if p != 0 {
                let o = read_addr(p, 4);
                let o2 = read_addr(p + 0x154, 4);

                m.push(o);
                m.push(o2);
            }
        }

        m.push(w);

        m.push(p4.to_addr());

        let p5 = read_vec(new + 0x20);
        m.push(p5.read_underlying());
        m.push(p5.to_addr());

        let p6 = read_linked_list(new + 0x30);
        m.extend(p6.read_all(0));

        read_bullets(new + 0x5c, char, m);

        let p8 = read_maybe_ring_buffer(player + 0x7b0);
        m.extend(p8.read_whole(0x10));

        let p9 = read_maybe_ring_buffer(player + 0x5e8);
        m.extend(p9.read_whole(0x98));

        let p10 = read_maybe_ring_buffer(player + 0x5b0);
        m.extend(p10.read_whole(0x10));

        let p11 = read_maybe_ring_buffer(player + 0x5fc);
        m.extend(p11.read_whole(0x10));
    }

    let get_player = |p_game_manager: usize, offset: usize| {
        assert!(offset < 4);
        if *((p_game_manager + 0x38 + offset) as *const u8) != 0 {
            Some(*((p_game_manager + 0x28 + offset * 4) as *const usize))
        } else {
            None
        }
    };

    let p_game_manager = read_addr(0x8985dc, 0x4);
    let p_game_manager = get_ptr(&p_game_manager.content[0..4], 0);

    m.push(read_addr(p_game_manager, 0x58));
    m.push(read_vec(p_game_manager + 0x40).read_underlying());

    let p1 = get_player(p_game_manager, 0).unwrap();
    read_player_data(p1, &mut m);

    let p2 = get_player(p_game_manager, 1).unwrap();
    read_player_data(p2, &mut m);

    // dumping characters (players) data for 2v2 mod
    get_player(p_game_manager, 2).and_then(|p| Some(read_player_data(p, &mut m)));
    get_player(p_game_manager, 3).and_then(|p| Some(read_player_data(p, &mut m)));

    assert_eq!(*((p_battle_manager + 0xc + 0 * 4) as *const usize), p1);
    assert_eq!(*((p_battle_manager + 0xc + 1 * 4) as *const usize), p2);

    #[cfg(feature = "logtofile")]
    if ISDEBUG {
        info!("bullets done");
    }

    m.push(read_addr(0x898718, 0x128));

    let sc1 = *(0x89881c as *const usize);
    // not sure what this is
    if sc1 != 0 {
        m.push(read_addr(sc1, 0x50));

        let sc2 = read_maybe_ring_buffer(sc1 + 0x3c);
        let z = sc2.obj_s as i32;

        #[cfg(feature = "logtofile")]
        if ISDEBUG {
            info!("weird deque done");
        }

        if z != 0 {
            let size = sc2.size as i32;
            let ptr = sc2.data as i32;

            let z = {
                let y = (sc2.f3 as i32 - 1 + z) % (size * 8);
                (ptr + ((y + (((y >> 0x1f) * 7) & 7)) >> 3)) as i32
            };

            let w = if ptr <= z - 0x50 { z - 0x50 } else { ptr };

            let x = (ptr + size).min(w + 0x28);

            m.push(read_addr(w as usize, (((x - w) >> 2) * 4) as usize));
        }
    }

    let to_be_read = [
        (0x898600, 0x6c),
        (0x8985d8, 4),
        (0x8971b8, 0x20),
        (0x8971d8, 0x1400), // 0x8971d8 ~ 0x8985d8
        (0x883cc8, 4),
        (0x89a88c, 4),
        (0x89a454, 4),
        (0x896d64, 8),
        (0x896b20, 4),
        (0x89b65c, 4),
        (0x89b660, 0x9c0),
        (0x89c01c, 4),
        (0x89aaf8, 4),
        (0x88526c, 4),
    ];

    for (pos, size) in to_be_read {
        let x = read_addr(pos, size);

        m.push(x);
    }

    // For F1, F5, F6 and F7
    m.push(read_addr(*(0x008971c8 as *mut usize) + 4, 8));

    let mut extra_states: Vec<ExtraState> = Vec::with_capacity(CALLBACK_ARRAY.len());

    for cb in CALLBACK_ARRAY.iter() {
        let i = (cb.save_state)();

        extra_states.push(ExtraState { cb: *cb, state: i })
    }

    // aligned to 4
    let buf_size: usize = m
        .iter()
        .map(|x| x.content.metadata.size.div_ceil(4) * 4)
        .sum();

    let mut buf = Vec::with_capacity(buf_size);
    for addr in &m {
        buf.extend_from_slice(&addr.content);
        buf.resize(buf.len().div_ceil(4) * 4, 0);
    }
    assert_eq!(buf_size, buf.len());

    LAST_M_LEN = m.len();

    let mut alloc: Vec<usize> = MEMORY_RECEIVER_ALLOC.as_ref().unwrap().try_iter().collect();
    let mut frees: Vec<usize> = MEMORY_RECEIVER_FREE.as_ref().unwrap().try_iter().collect();
    extra_allocs.and_then(|x| Some(alloc.extend(x)));
    extra_frees.and_then(|x| Some(frees.extend(x)));

    let f = Frame {
        number: *SOKU_FRAMECOUNT,
        addresses: m.into_iter().map(|x| x.content.metadata).collect(),
        addresses_buf: buf.into_boxed_slice(),
        fp: w,
        frees: MEMORY_RECEIVER_FREE.as_ref().unwrap().try_iter().collect(),
        allocs: MEMORY_RECEIVER_ALLOC.as_ref().unwrap().try_iter().collect(),
        extra_states,
        weather_sync_check: ((*(0x8971c4 as *const usize) * 16) + (*(0x8971c4 as *const usize) * 1)
            & 0xFF) as u8,
        has_happened: false,
        has_called_never_happened: false,
        last_shake_before_smooth: LAST_CAMERA_BEFORE_SMOOTH.clone(),
    };
    if let Some(time) = &mut DUMP_FRAME_TIME
        && let Some(now) = now
    {
        *time += now.elapsed();
    }
    f
}

pub fn read_heap(pos: usize) -> usize {
    unsafe {
        windows::Win32::System::Memory::HeapSize(
            HANDLE(*(0x89b404 as *const isize)),
            windows::Win32::System::Memory::HEAP_FLAGS(0),
            pos as *const c_void,
        )
    }
}

#[derive(Debug)]
pub struct ReadAddrMetadata {
    size: usize,
    pos: *mut u8,
}

unsafe impl Send for ReadAddrMetadata {}

#[derive(Debug)]
struct ReadAddrContent {
    pub metadata: ReadAddrMetadata,
}

#[derive(Debug)]
struct ReadAddr {
    pub content: ReadAddrContent,
}

impl Deref for ReadAddrContent {
    type Target = [u8];

    fn deref<'a>(&'a self) -> &'a Self::Target {
        unsafe { std::slice::from_raw_parts(self.metadata.pos, self.metadata.size) }
    }
}

impl ReadAddr {
    fn usize_align<'a>(&'a self) -> &'a [usize] {
        unsafe { self.usize_align_() }
    }

    unsafe fn usize_align_<'a>(&self) -> &'a [usize] {
        // Soku2 <= v2.30f only unaligned on stack. So it should be ok.
        assert!((self.content.metadata.pos as *const usize).is_aligned());
        assert_eq!(self.content.metadata.size % 4, 0);
        unsafe {
            std::slice::from_raw_parts(
                self.content.metadata.pos as *const usize,
                self.content.metadata.size / 4,
            )
        }
    }
}

#[derive(Debug)]
struct VecAddr {
    pub pos: usize,
    pub start: usize,
    pub maybecapacity: usize,
    pub end: usize,
}

#[derive(Debug)]
struct LL4 {
    pub pos: usize,
    pub next: usize,
    #[allow(unused)]
    pub field2: usize,
    pub additional_data: usize,
}

impl LL4 {
    fn to_addr(&self) -> ReadAddr {
        read_addr(self.pos, 12)
    }

    fn read_underlying_additional(&self, size: usize) -> ReadAddr {
        let ret = read_addr(self.additional_data, size);

        ret
    }
}

#[derive(Debug)]
struct LL3Holder {
    pub pos: usize,
    pub ll4: usize,
    pub listcount: usize,
    #[allow(unused)]
    pub add_data: usize,
}

use core::iter::from_coroutine;

impl LL3Holder {
    fn read_underlying<'a>(&'a self) -> impl Iterator<Item = LL4> + 'a {
        if self.ll4 == 0 {
            #[cfg(feature = "logtofile")]
            info!("ll4 is 0 ,painc");
            panic!("ll4 is 0");
        }
        let c = #[coroutine] || {
            let last = read_ll4(self.ll4);
            let mut last_next = last.next;
            yield last;

            if self.listcount > 100000 {
                panic!("list too big");
            }

            for _ in 0..self.listcount {
                let next = last_next;
                if next == 0 {
                    //#[cfg(feature = "logtofile")]
                    //info!(
                    //    "next was equal to zero, pos {}, out of {}",
                    //    a,
                    //    self.listcount - 1
                    //);
                    panic!();
                };
                let last = read_ll4(next);
                last_next = last.next;
                yield last
            }
        };
        from_coroutine(c)
    }

    fn read_all<'a>(&'a self, additional_size: usize) -> impl Iterator<Item = ReadAddr> + 'a {
        //I think that readLL3 does not read itself, however, I will leave this here because it cannot hurt
        let c = #[coroutine] move || {
            yield self.to_addr();
            if self.listcount == 0 {
                yield read_ll4(self.ll4).to_addr();
            } else {
                let size = additional_size;
                if size == 0 {
                    for ll4 in self.read_underlying() {
                        yield ll4.to_addr();
                    }
                } else {
                    let mut uv = self.read_underlying();
                    yield uv.next().unwrap().to_addr();

                    for ll4 in uv {
                        yield ll4.to_addr();
                        if ll4.additional_data != 0 {
                            yield ll4.read_underlying_additional(size);
                        }
                    }
                }
            };
        };
        from_coroutine(c)
    }

    fn to_addr(&self) -> ReadAddr {
        read_addr(self.pos, 12)
    }
}

impl VecAddr {
    fn read_underlying(&self) -> ReadAddr {
        read_addr(self.start, self.end - self.start)
    }

    fn to_addr(&self) -> ReadAddr {
        read_addr(self.pos, 12)
    }
}
#[derive(Debug)]
struct Deque {
    #[allow(unused)]
    pos: usize,
    #[allow(unused)]
    f0: usize,
    data: usize,
    size: usize,
    f3: usize,
    obj_s: usize,
}

impl Deque {
    #[allow(unused)]
    fn to_addr(&self) -> ReadAddr {
        read_addr(self.pos, 20)
    }

    fn read_underlying<'a>(&'a self, size: usize) -> impl Iterator<Item = ReadAddr> + 'a {
        let unknown: ReadAddr = read_addr(self.data, self.size * 4);

        match size {
            0 => &[],
            _ => unsafe { unknown.usize_align_() },
        }
        .into_iter()
        .filter(|x| **x != 0)
        .map(move |x| read_addr(*x, size))
        .chain([unknown].into_iter())
    }

    fn read_whole<'a>(&'a self, size: usize) -> impl Iterator<Item = ReadAddr> + 'a {
        self.read_underlying(match self.obj_s {
            0 => 0,
            _ => size,
        })
    }
}

#[must_use]
fn read_addr(pos: usize, size: usize) -> ReadAddr {
    if size > 10000 {
        panic!("size too big {}", size);
    }
    assert!(!(pos == 0 && size != 0));
    if size == 0 {
        #[cfg(feature = "logtofile")]
        if ISDEBUG {
            info!("unchecked 0 addr read")
        };
        return ReadAddr {
            content: ReadAddrContent {
                // to create a reference, pos should be non-null and aligned
                metadata: ReadAddrMetadata {
                    pos: 4 as _,
                    size: 0,
                },
            },
        };
    }

    return ReadAddr {
        content: ReadAddrContent {
            metadata: ReadAddrMetadata {
                pos: pos as *mut u8,
                size: size,
            },
        },
    };
}
#[must_use]
fn read_vec(pos: usize) -> VecAddr {
    let ptr = pos as *const u8;
    let content = unsafe { std::slice::from_raw_parts(ptr, 12) };

    VecAddr {
        pos: pos,
        start: usize::from_le_bytes(content[0..4].try_into().unwrap()),
        maybecapacity: usize::from_le_bytes(content[4..8].try_into().unwrap()),
        end: usize::from_le_bytes(content[8..12].try_into().unwrap()),
    }
}
#[must_use]
fn read_linked_list(pos: usize) -> LL3Holder {
    let w = read_addr(pos, 12);

    LL3Holder {
        pos: pos,

        ll4: usize::from_le_bytes(w.content[0..4].try_into().unwrap()),
        listcount: usize::from_le_bytes(w.content[4..8].try_into().unwrap()),
        add_data: usize::from_le_bytes(w.content[8..12].try_into().unwrap()),
    }
}
#[must_use]
fn read_ll4(pos: usize) -> LL4 {
    let w = read_addr(pos, 12);
    LL4 {
        pos: pos,
        next: usize::from_le_bytes(w.content[0..4].try_into().unwrap()),
        field2: usize::from_le_bytes(w.content[4..8].try_into().unwrap()),
        additional_data: usize::from_le_bytes(w.content[8..12].try_into().unwrap()),
    }
}

#[must_use]
fn read_maybe_ring_buffer(pos: usize) -> Deque {
    let m = read_addr(pos, 20);

    let w = m.usize_align();

    Deque {
        pos: pos,
        f0: w[0],
        data: w[1],
        size: w[2],
        f3: w[3],
        obj_s: w[4],
    }
}

fn get_ptr(from: &[u8], offset: usize) -> usize {
    usize::from_le_bytes(from[offset..offset + 4].try_into().unwrap())
}

#[derive(Debug)]
pub struct ExtraState {
    cb: Callbacks,
    state: u32,
}

#[derive(Debug)]
pub struct Frame {
    pub number: usize,
    pub addresses: Box<[ReadAddrMetadata]>,
    pub addresses_buf: Box<[u8]>,
    pub fp: [u8; 108],
    pub frees: Vec<usize>,
    pub allocs: Vec<usize>,
    pub extra_states: Vec<ExtraState>,

    pub weather_sync_check: u8,
    pub has_called_never_happened: bool,
    pub has_happened: bool,
    pub last_shake_before_smooth: Option<CameraTransform>,
}

impl Drop for Frame {
    fn drop(&mut self) {
        if !(self.has_called_never_happened || self.has_happened) {
            println!("WARNING (fix me): this frame ({}) neither `did_happen` nor `never_happened`, so all its allocated memory leaks", self.number);
        }
    }
}

impl Frame {
    pub fn never_happened(&mut self) -> (HashSet<usize>, HashSet<usize>) {
        assert!(!self.has_happened);
        self.has_called_never_happened = true;
        let mut allocs: HashSet<usize> = self.allocs.iter().map(|x| *x).collect();
        let mut frees: HashSet<usize> = self.frees.iter().map(|x| *x).collect();

        let alloc_then_free: Vec<usize> = allocs.intersection(&frees).map(|x| *x).collect();
        for a in alloc_then_free {
            #[cfg(feature = "logtofile")]
            if ISDEBUG {
                info!("alloc: {}", a)
            };
            allocs.remove(&a);
            frees.remove(&a);
            unsafe { soku_heap_free!(a) };
        }
        self.allocs.retain(|x| allocs.contains(x));
        self.frees.retain(|x| frees.contains(x));

        // for a in allocs.difference(&frees) {
        //     //println!("never freed: {}", a);
        // }
        for a in self.extra_states.iter() {
            unsafe {
                (a.cb.free_state)(a.state, true);
            }
        }
        (allocs, frees)
    }

    pub fn did_happen(&mut self) {
        //let m = &mut *ALLOCMUTEX.lock().unwrap();
        //
        //for a in self.frees.iter() {
        //    m.remove(a);
        //}
        assert!(!self.has_called_never_happened);
        self.has_happened = true;

        for a in &self.frees {
            unsafe { soku_heap_free!(*a) };
        }
        for a in self.extra_states.iter() {
            unsafe {
                (a.cb.free_state)(a.state, false);
            }
        }
        self.frees.clear();
        self.allocs.clear();
    }

    #[allow(unused)]
    fn size_data(&self) -> String {
        let addr_total = self.addresses.iter().fold(0, |a, x| a + x.size);
        let freetotal = self.frees.len() * 4;
        let alloctotal = self.allocs.len() * 4;
        format!("addr: {addr_total} frees: {freetotal} allocs: {alloctotal}")
    }

    #[allow(unused)]
    fn redundency_data(&self) -> String {
        let mut w = HashSet::new();
        let mut counter = 0;
        for a in self.addresses.iter() {
            for b in 0..a.size {
                if !w.insert(a.pos as usize + b) {
                    counter += 1;
                }
            }
        }

        format!("reduntant bytes: {}", counter)
    }

    pub fn restore(
        &self,
        dropped_frames: Option<impl Iterator<Item = &mut Frame>>,
        extra_allocs: Option<impl Iterator<Item = usize>>,
        extra_frees: Option<impl Iterator<Item = usize>>,
    ) {
        assert_ne!(self.number, 0);
        assert!(!self.has_called_never_happened);
        unsafe {
            FPST = self.fp;
            asm!(
                "FRSTOR {fpst}",
                fpst = sym FPST
            )
        }
        #[cfg(feature = "logrollback")]
        println!("restore {}", self.number);
        let mut allocs: HashSet<usize> = HashSet::new();
        fn free_if_allocated(
            allocs: &mut HashSet<usize>,
            new_allocs: Option<impl Iterator<Item = usize>>,
            new_frees: Option<impl Iterator<Item = usize>>,
        ) {
            if let Some(new_allocs) = new_allocs {
                allocs.extend(new_allocs);
            }
            if let Some(new_frees) = new_frees {
                for a in new_frees {
                    if allocs.contains(&a) {
                        unsafe { soku_heap_free!(a) };
                        allocs.remove(&a);
                    }
                }
            }
        }
        if let Some(dropped_frames) = dropped_frames {
            let mut last_frame = self.number;
            for f in dropped_frames {
                assert!(last_frame < f.number);
                last_frame = f.number;
                let (_, __) = f.never_happened();
                free_if_allocated(
                    &mut allocs,
                    Some(f.allocs.iter().map(|x| *x)),
                    Some(f.frees.iter().map(|x| *x)),
                );
                f.allocs.clear();
                f.frees.clear();
            }
        }
        free_if_allocated(&mut allocs, extra_allocs, extra_frees);
        unsafe {
            free_if_allocated(
                &mut allocs,
                Some(MEMORY_RECEIVER_ALLOC.as_ref().unwrap().try_iter()),
                Some(MEMORY_RECEIVER_FREE.as_ref().unwrap().try_iter()),
            );
        }
        unsafe {
            MEMORY_LEAK += allocs
                .into_iter()
                .map(|x| read_heap(x).div_ceil(8) * 8)
                .sum::<usize>()
        };

        for a in self.extra_states.iter() {
            unsafe {
                (a.cb.load_state_pre)(self.number, a.state);
            }
        }
        let mut index = 0;
        for a in self.addresses.iter() {
            let new_index = index + a.size.div_ceil(4) * 4;
            if a.pos != null_mut() {
                unsafe {
                    a.pos
                        .copy_from(self.addresses_buf[index..new_index].as_ptr(), a.size);
                }
            }
            index = new_index;
        }
        assert_eq!(self.addresses_buf.len(), index);
        for a in self.extra_states.iter() {
            unsafe {
                (a.cb.load_state_post)(a.state);
            }
        }
        unsafe {
            LAST_CAMERA_BEFORE_SMOOTH = self.last_shake_before_smooth.clone();
        }
    }
}

/*

unsafe fn deasm() {
    asm!(
        "PUSH       EBX",
        "PUSH       ESI",
        "MOV        ESI,param_1",
        "PUSH       EDI",
        "MOV        EBX,ESI",
        "CALL       FUN_100027d0",
        "MOV        EDI,0x6c",
        "MOV        ECX,0x898600",
        "CALL       readGameData",
        "MOV        EDI,0x4",
        "MOV        ECX=>Framecount2,0x8985d8",
        "CALL       readGameData",
        "MOV        ECX,0x8985d4",
        "CALL       readGameData",
        "MOV        EDI,0x20",
        "MOV        ECX,0x8971b8",
        "CALL       readGameData",
        "MOV        EDI,0x4",
        "MOV        ECX,0x883cc8",
        "CALL       readGameData",
        "MOV        ECX=>DAT_0089a88c,0x89a88c",
        "CALL       readGameData",
        "MOV        ECX,0x89a454",
        "CALL       readGameData",
        "MOV        EDI,0x8",
        "MOV        ECX,0x896d64",
        "CALL       readGameData",
        "MOV        EDI,0x4",
        "MOV        ECX=>DAT_00896b20,0x896b20",
        "CALL       readGameData",
        "MOV        ECX,0x89b65c",
        "CALL       readGameData",
        "MOV        EDI,0x9c0",
        "MOV        ECX,0x89b660",
        "CALL       readGameData",
        "MOV        EDI,0x4",
        "MOV        ECX,0x89c01c",
        "CALL       readGameData",
        "MOV        ECX,0x89aaf8",
        "CALL       readGameData",
        "MOV        ECX,0x88526c",
        "CALL       readGameData",
        "MOV        EDI,0x14",
        "MOV        ECX,0x8971c0",
        "CALL       readGameData",
        "MOV        ECX,dword ptr [DAT_008971c8]",
        "MOV        EDI,0x4c",
        "CALL       readGameData",
        "MOV        EBX,dword ptr [DAT_008985ec]",
        "MOV        EDI,0xec",
        "MOV        ECX,EBX",
        "CALL       readGameData",
        "LEA        param_1,[EBX + 0x1c]",
        "CALL       store_autosize",
        "LEA        param_1,[EBX + 0x68]",
        "CALL       store_autosize",
        "PUSH       0x0",
        "LEA        param_1,[EBX + 0x78]",
        "PUSH       param_1",
        "MOV        param_1,ESI",
        "CALL       readLL3",
        "PUSH       0x180",
        "LEA        ECX,[EBX + 0xa4]",
        "PUSH       ECX",
        "MOV        param_1,ESI",
        "CALL       readLL3",
        "LEA        param_1,[EBX + 0x28]",
        "MOV        ECX,ESI",
        "CALL       ReadWeirderLinkedList",
        "MOV        EBX,dword ptr [DAT_008985e0]",
        "MOV        EDI,0x118",
        "MOV        ECX,EBX",
        "CALL       readGameData",
        "PUSH       0x0",
        "LEA        EDX,[EBX + 0x4]",
        "PUSH       EDX",
        "MOV        param_1,ESI",
        "CALL       readLL3",
        "LEA        param_1,[EBX + 0x2c]",
        "PUSH       param_1",
        "PUSH       ESI",
        "CALL       ReadSizeDetect",
        "ADD        ESP,0x8",
        "PUSH       0x0",
        "ADD        EBX,0x38",
        "PUSH       EBX",
        "MOV        param_1,ESI",
        "CALL       readLL3",
        "MOV        EBX,dword ptr [DAT_008985f0]",
        "MOV        EDI,0x94",
        "MOV        ECX,EBX",
        "CALL       readGameData",
        "LEA        param_1,[EBX + 0x10]",
        "CALL       store_autosize",
        "LEA        param_1,[EBX + 0x20]",
        "CALL       store_autosize",
        "PUSH       0x0",
        "LEA        ECX,[EBX + 0x30]",
        "PUSH       ECX",
        "MOV        param_1,ESI",
        "CALL       readLL3",
        "PUSH       0x178",
        "ADD        EBX,0x5c",
        "PUSH       EBX",
        "MOV        param_1,ESI",
        "CALL       readLL3",
        "MOV        EBX,dword ptr [DAT_008985e8]",
        "MOV        EDI,0x688",
        "MOV        ECX,EBX",
        "CALL       readGameData",
        "LEA        param_1,[EBX + 0x14]",
        "CALL       store_autosize",
        "LEA        param_1,[EBX + 0x24]",
        "CALL       store_autosize",
        "PUSH       0x0",
        "LEA        EDX,[EBX + 0x34]",
        "PUSH       EDX",
        "MOV        param_1,ESI",
        "CALL       readLL3",
        "PUSH       0x178",
        "LEA        param_1,[EBX + 0x60]",
        "PUSH       param_1",
        "MOV        param_1,ESI",
        "CALL       readLL3",
        "LEA        param_1,[EBX + 0x18c]",
        "MOV        ECX,ESI",
        "CALL       ReadLinkedListWrappingBig",
        "LEA        param_1,[EBX + 0x1c0]",
        "MOV        ECX,ESI",
        "CALL       ReadLinkedListWrappingBig",
        "MOV        EBX,dword ptr [DAT_008985e4]",
        "MOV        EDI,0x908",
        "MOV        ECX,EBX",
        "CALL       readGameData",
        "PUSH       0x0",
        "LEA        ECX,[EBX + 0x30]",
        "PUSH       ECX",
        "MOV        param_1,ESI",
        "CALL       readLL3",
        "PUSH       0x0",
        "LEA        EDX,[EBX + 0x3c]",
        "PUSH       EDX",
        "MOV        param_1,ESI",
        "CALL       readLL3",
        "PUSH       0x0",
        "LEA        param_1,[EBX + 0x48]",
        "PUSH       param_1",
        "MOV        param_1,ESI",
        "CALL       readLL3",
        "PUSH       0x0",
        "LEA        ECX,[EBX + 0x54]",
        "PUSH       ECX",
        "MOV        param_1,ESI",
        "CALL       readLL3",
        "PUSH       0x0",
        "LEA        EDX,[EBX + 0x60]",
        "PUSH       EDX",
        "MOV        param_1,ESI",
        "CALL       readLL3",
        "PUSH       0x0",
        "LEA        param_1,[EBX + 0x6c]",
        "PUSH       param_1",
        "MOV        param_1,ESI",
        "CALL       readLL3",
        "LEA        param_1,[EBX + 0x9c]",
        "CALL       store_autosize",
        "LEA        param_1,[EBX + 0xac]",
        "CALL       store_autosize",
        "PUSH       0x0",
        "LEA        ECX,[EBX + 0xbc]",
        "PUSH       ECX",
        "MOV        param_1,ESI",
        "CALL       readLL3",
        "PUSH       0x0",
        "ADD        EBX,0xe8",
        "PUSH       EBX",
        "MOV        param_1,ESI",
        "CALL       readLL3",
        "MOV        EBX,dword ptr [DAT_008985dc]",
        "MOV        EDI,0x58",
        "MOV        ECX,EBX",
        "CALL       readGameData",
        "LEA        param_1,[EBX + 0x40]",
        "CALL       store_autosize",
        "XOR        ECX,ECX",
        "MOV        param_1,ESI",
        "CALL       FUN_100180f0",
        "LEA        ECX,[EDI + -0x57]",
        "MOV        param_1,ESI",
        "CALL       FUN_100180f0",
        "MOV        EDX,dword ptr [Mutex1]",
        "MOV        EBX,dword ptr [DAT_008986a0]",
        "PUSH       -0x1",
        "LEA        ECX,[EBX + 0xf8]",
        "MOV        EDI,0x68",
        "CALL       readGameData",
        "LEA        ECX,[EBX + 0x174]",
        "CALL       readGameData",
        "MOV        EBX,dword ptr [DAT_0089881c]",
        "MOV        EDI,0x128",
        "MOV        ECX,0x898718",
        "CALL       readGameData",
        "MOV        EDI,0x50",
        "MOV        ECX,EBX",
        "CALL       readGameData",
        "LEA        param_1,[EBX + 0x3c]",
        "PUSH       ESI",
        "CALL       FUN_100026f0",
        "POP        EDI",
        "POP        ESI",
        "POP        EBX",
        "RET",
    )
}
 */
