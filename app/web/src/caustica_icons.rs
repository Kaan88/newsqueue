//! Caustica vector icons and the approved Sea Glass / Copper palettes.
use crate::theme::Icon;
use egui::{Color32, Painter, Rect, Pos2, Shape, Stroke, vec2};
use std::sync::atomic::{AtomicBool, Ordering};
static COPPER: AtomicBool = AtomicBool::new(false);
pub fn configure(copper: bool) -> bool {
    COPPER.swap(copper, Ordering::Relaxed) != copper
}
fn rgb(v: u32) -> Color32 { Color32::from_rgb((v >> 16) as u8,(v >> 8) as u8,v as u8) }
fn pair(light:u32,dark:u32)->Color32 { if crate::caustica::is_light() {rgb(light)} else {rgb(dark)} }
pub fn accent()->Color32 { if COPPER.load(Ordering::Relaxed) {pair(0x875629,0xd6ad83)} else {pair(0x246963,0x6fbaaf)} }
fn teal()->Color32 {if COPPER.load(Ordering::Relaxed) {pair(0x236f69,0x75c8bc)} else {pair(0x246963,0x6fbaaf)}}
fn amber()->Color32 {pair(0x8e602e,0xd6ad83)}
fn red()->Color32 {pair(0xa44343,0xd98d88)}
pub fn color(name:&str)->Option<Color32> {Some(match name {
    "BAR"=>teal(), "LINK"=>accent(),
    "SEL"=>if COPPER.load(Ordering::Relaxed) {pair(0xecdfce,0x493b30)} else {pair(0xd4e8e2,0x28443f)},
    "GREEN"=>pair(0x397651,0x91bc9c), "RED"=>red(), "ORANGE"=>amber(), "BLUE"=>teal(),
    _=>return None,
})}
struct Pen<'a>{p:&'a Painter,r:Rect,k:f32,ink:Color32,accent:Color32}
impl Pen<'_>{
    fn pt(&self,x:f32,y:f32)->Pos2 {self.r.min+vec2(x*self.k,y*self.k)}
    fn line(&self,points:&[(f32,f32)],c:Color32){self.p.add(Shape::line(points.iter().map(|&(x,y)|self.pt(x,y)).collect(),Stroke::new((1.55*self.k).max(1.0),c)));}
    fn box_(&self,x:f32,y:f32,w:f32,h:f32,c:Color32){let r=Rect::from_min_size(self.pt(x,y),vec2(w*self.k,h*self.k));self.p.rect_filled(r,2.0*self.k,c.gamma_multiply(0.10));self.p.rect_stroke(r,2.0*self.k,Stroke::new((1.4*self.k).max(1.0),c),egui::StrokeKind::Inside);}
    fn circle(&self,x:f32,y:f32,r:f32,c:Color32){self.p.circle_stroke(self.pt(x,y),r*self.k,Stroke::new((1.5*self.k).max(1.0),c));}
    fn dot(&self,x:f32,y:f32,c:Color32){self.p.circle_filled(self.pt(x,y),1.1*self.k,c);}
    fn arc(&self,x:f32,y:f32,r:f32,a:f32,b:f32,c:Color32){self.line(&(0..=20).map(|i|{let t=a+(b-a)*i as f32/20.0;(x+r*t.cos(),y+r*t.sin())}).collect::<Vec<_>>(),c);}
}
pub fn icon(p:&Painter,r:Rect,icon:Icon,enabled:bool){
    let neutral=if COPPER.load(Ordering::Relaxed) {pair(0x4c5b5b,0xb5c2c1)} else {pair(0x4c5958,0xaebbb9)};
    let tint=|c|if enabled{c}else{pair(0x9ca5a4,0x66716f)};
    let g=Pen{p,r,k:r.width().min(r.height())/24.0,ink:tint(neutral),accent:tint(accent())};
    let a=g.accent;let n=g.ink;let warn=tint(amber());let danger=tint(red());
    use Icon::*;
    match icon {
        Nzb|File|Log|Rename=>{
            g.line(&[(5.,3.),(14.,3.),(19.,8.),(19.,21.),(5.,21.),(5.,3.)],n);
            g.line(&[(14.,3.),(14.,8.),(19.,8.)],n);
            match icon {
                Nzb=>{g.line(&[(12.,10.),(12.,17.)],a);g.line(&[(9.,14.),(12.,17.),(15.,14.)],a);},
                Rename=>{g.line(&[(9.,17.),(16.,10.),(18.,12.),(11.,19.),(8.,20.),(9.,17.)],a);},
                _=>{g.line(&[(8.,12.),(16.,12.)],a);g.line(&[(8.,16.),(14.,16.)],n);}
            }
        },
        Connect|Link=>{g.box_(3.,8.,7.,8.,n);g.box_(14.,8.,7.,8.,n);g.line(&[(8.,12.),(16.,12.)],a);g.dot(6.5,12.,a);g.dot(17.5,12.,a);},
        Pause|PauseFor=>{g.box_(5.,5.,4.,14.,if icon==Pause{a}else{n});g.box_(13.,5.,4.,14.,if icon==Pause{a}else{n});if icon==PauseFor {g.circle(17.,16.,5.,warn);g.line(&[(17.,13.),(17.,16.),(20.,17.)],warn);}},
        Speed=>{g.arc(12.,15.,8.,std::f32::consts::PI,2.0*std::f32::consts::PI,n);g.line(&[(5.,18.),(19.,18.)],n);g.line(&[(12.,15.),(17.,8.)],a);g.dot(12.,15.,a);},
        Top|Up|Down|Bottom=>{let up=matches!(icon,Top|Up);let y=|v|if up{v}else{24.-v};g.line(&[(6.,y(12.)),(12.,y(6.)),(18.,y(12.))],a);g.line(&[(12.,y(6.)),(12.,y(20.))],n);if matches!(icon,Top|Bottom){g.line(&[(5.,y(3.)),(19.,y(3.))],n);}},
        Delete=>{g.line(&[(6.,7.),(7.,21.),(17.,21.),(18.,7.)],n);g.line(&[(4.,6.),(20.,6.)],danger);g.line(&[(9.,5.),(9.,3.),(15.,3.),(15.,5.)],danger);g.line(&[(10.,10.),(10.,17.)],n);g.line(&[(14.,10.),(14.,17.)],n);},
        Cleanup=>{g.line(&[(17.,3.),(10.,13.)],n);g.line(&[(9.,11.),(15.,15.),(12.,21.),(3.,17.),(9.,11.)],a);g.line(&[(7.,16.),(5.,19.)],n);g.line(&[(10.,17.),(8.,20.)],n);g.dot(19.,16.,warn);g.dot(20.,20.,warn);},
        Unpack|Folder=>{g.line(&[(3.,7.),(3.,5.),(9.,5.),(12.,8.),(21.,8.),(19.,20.),(4.,20.),(3.,7.)],n);g.line(&[(4.,10.),(19.,10.)],a);if icon==Unpack{g.line(&[(12.,11.),(12.,17.)],a);g.line(&[(9.,14.),(12.,17.),(15.,14.)],a);}},
        Setup=>{for (x,y) in [(5.,8.),(12.,16.),(19.,10.)]{g.line(&[(x,3.),(x,21.)],n);g.box_(x-2.5,y-2.,5.,4.,a);}},
        History|Refresh=>{g.arc(12.,12.,8.,-2.6,2.4,n);g.line(&[(4.,3.),(4.,8.),(9.,8.)],a);if icon==History{g.line(&[(12.,7.),(12.,12.),(16.,14.)],a);}else{g.line(&[(20.,21.),(20.,16.),(15.,16.)],a);}},
        Par2=>{g.line(&[(12.,3.),(20.,6.),(19.,15.),(16.,19.),(12.,22.),(8.,19.),(5.,15.),(4.,6.),(12.,3.)],n);g.line(&[(8.,12.),(11.,15.),(16.,9.)],a);},
        Rss=>{g.dot(5.,19.,warn);g.arc(5.,19.,7.,-1.57,0.,n);g.arc(5.,19.,13.,-1.57,0.,warn);},
        Play=>g.line(&[(7.,4.),(20.,12.),(7.,20.),(7.,4.)],a),
        Stats=>{g.line(&[(3.,3.),(3.,21.),(22.,21.)],n);g.box_(6.,13.,3.,6.,a);g.box_(12.,7.,3.,12.,a);g.box_(18.,10.,3.,9.,warn);},
        Key=>{g.circle(8.,9.,5.,a);g.line(&[(12.,13.),(21.,21.),(22.,19.),(19.,16.),(17.,18.)],n);g.dot(7.,8.,a);},
        Queue=>{for y in [6.,12.,18.]{g.dot(4.,y,a);g.line(&[(8.,y),(21.,y)],n);}},
        Check=>g.line(&[(4.,12.),(10.,18.),(21.,5.)],a),
        Error=>{g.circle(12.,12.,9.,danger);g.line(&[(8.,8.),(16.,16.)],danger);g.line(&[(16.,8.),(8.,16.)],danger);},
        Warning=>{g.line(&[(12.,3.),(22.,21.),(2.,21.),(12.,3.)],warn);g.line(&[(12.,9.),(12.,14.)],warn);g.dot(12.,18.,warn);},
        Info|Question=>{g.circle(12.,12.,9.,n);if icon==Info {g.dot(12.,7.,a);g.line(&[(12.,11.),(12.,18.)],a);}else{g.arc(12.,9.,3.,-3.14,1.57,a);g.line(&[(12.,12.),(12.,14.)],a);g.dot(12.,18.,a);}},
    }
}
