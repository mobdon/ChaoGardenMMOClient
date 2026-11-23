//! Sonic's normal-course crouch, rolling friction and spin-dash charge.
//! The caller performs the original terrain probes and sprite callbacks.
use crate::{Error,VerifiedRom,native_player::{self as player,PlayerState},native_runtime::{DPAD_DOWN,DPAD_LEFT,DPAD_RIGHT}};
pub const CROUCH:i16=2;
pub const DASH:i16=3;
pub const ROLL:i16=4;
pub const CHARGING:u32=0x400;

#[derive(PartialEq,Eq,Debug,Clone,Copy)]
pub enum DownAction {Crouch,Roll}
pub fn down_action(p:&PlayerState)->Option<DownAction>{
    if p.held_input&0xf0!=DPAD_DOWN || p.move_state & (0x1000000|player::SPIN_ATTACK|player::IN_AIR)!=0{return None;}
    if p.q_speed_ground==0 && (p.rotation.wrapping_add(32)&0xc0)==0 {return Some(DownAction::Crouch)}
    (p.q_speed_ground.wrapping_add(127) as u16>254).then_some(DownAction::Roll)
}
pub fn roll_speed(p:&mut PlayerState, sin:i32)->bool {
    if p.control_lock==0{
        match p.held_input&(DPAD_LEFT|DPAD_RIGHT){
            DPAD_LEFT if p.q_speed_ground<=0=>p.move_state|=player::FACING_LEFT,
            DPAD_LEFT=>p.q_speed_ground=if p.q_speed_ground<24{-96}else{p.q_speed_ground-24},
            DPAD_RIGHT if p.q_speed_ground>=0=>p.move_state&=!player::FACING_LEFT,
            DPAD_RIGHT=>p.q_speed_ground=if p.q_speed_ground > -24{96}else{p.q_speed_ground+24},
            _=>{}
        }
    }
    p.q_speed_ground-=p.q_speed_ground.signum()*8;
    if i32::from(p.q_speed_ground).abs()<128 {p.q_speed_ground=0;return false;}
    rolling_slope(p,sin);true
}
pub fn rolling_slope(p:&mut PlayerState,sin:i32){
    let v=p.q_speed_ground;
    if p.rotation.wrapping_add(96)<192 && v!=0 {
        let mut delta=(sin*60)>>8;
        if (v>0 && delta<=0)||(v<0 && delta>=0){delta>>=2;}
        p.q_speed_ground=v.wrapping_add(delta as i16);
    }
}
pub fn init_roll(p:&mut PlayerState){
    p.sprite_frame_flags&=!player::ANIMATION_OVER;p.char_state=ROLL;
    p.change_sprite_offsets(6,9,false);p.move_state|=player::SPIN_ATTACK;
}
#[derive(Debug)]
pub struct SpinDash {pub charge:i16, speeds:[i16;9]}
impl SpinDash{
    pub fn new(rom:&VerifiedRom<'_>)->Result<Self,Error>{
        let b=rom.range("rom",0xd6920,18)?;
        Ok(Self{charge:0,speeds:std::array::from_fn(|i|i16::from_le_bytes([b[i*2],b[i*2+1]]))})
    }
    pub fn start(&mut self,p:&mut PlayerState){
        p.char_state=DASH;p.move_state=(p.move_state|CHARGING|player::SPIN_ATTACK)&!(0x20|player::IN_AIR);
        p.change_sprite_offsets(6,9,false);self.charge=0;
        p.q_speed_air_x=0;p.q_speed_air_y=0;p.q_speed_ground=0;
    }
    pub fn tap_or_decay(&mut self,tap:bool)->Option<i16>{
        let mut next=self.charge;
        if next!=0 {next=next.wrapping_sub(((i32::from(next)*8)>>8) as i16).max(0);}
        let pitch=next&!127;
        // The original uses MAX, not an upper cap, on each new A edge.
        if tap{next=next.wrapping_add(512).max(2048);}
        self.charge=next;tap.then_some(pitch)
    }
    pub fn release(&self,p:&mut PlayerState){
        p.move_state &= !CHARGING;
        let speed=self.speeds[((self.charge>>8).clamp(0,8)) as usize];
        p.q_speed_ground=if p.move_state&player::FACING_LEFT!=0{-speed}else{speed};
    }
}
