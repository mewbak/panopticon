/*
 * Panopticon - A libre disassembler
 * Copyright (C) 2014-2015 Kai Michaelis
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/>.
 */

use std::collections::{HashMap,BTreeMap,BTreeSet};
use std::rc::Rc;

use graph_algos::{AdjacencyList,GraphTrait,MutableGraphTrait};
use graph_algos::adjacency_list::{
    AdjacencyListVertexDescriptor,
    AdjacencyListEdgeDescriptor,
};
use graph_algos::{VertexListGraphTrait,EdgeListGraphTrait};
use graph_algos::search::{
    TraversalOrder,
    TreeIterator,
};
use uuid::Uuid;

use {
    BasicBlock,
    Guard,
    Disassembler,
    Architecture,
    LayerIter,
    Rvalue,
    Mnemonic,
    Statement,
    Operation,
};

#[derive(RustcDecodable,RustcEncodable,Debug)]
pub enum ControlFlowTarget {
    Resolved(BasicBlock),
    Unresolved(Rvalue),
}

pub type ControlFlowGraph = AdjacencyList<ControlFlowTarget,Guard>;
pub type ControlFlowRef = AdjacencyListVertexDescriptor;
pub type ControlFlowEdge = AdjacencyListEdgeDescriptor;

#[derive(RustcDecodable,RustcEncodable)]
pub struct Function {
    pub uuid: Uuid,
    pub name: String,
    pub cflow_graph: ControlFlowGraph,
    pub entry_point: Option<ControlFlowRef>,
    pub region: String,
}

impl Function {
    pub fn new(a: String, reg: String) -> Function {
        Function{
            uuid: Uuid::new_v4(),
            name: a,
            cflow_graph: AdjacencyList::new(),
            entry_point: None,
            region: reg,
        }
    }

    pub fn with_uuid(a: String,uu: Uuid, reg: String) -> Function {
        Function{
            uuid: uu,
            name: a,
            cflow_graph: AdjacencyList::new(),
            entry_point: None,
            region: reg,
        }
    }

    fn index_cflow_graph(g: ControlFlowGraph) -> (BTreeMap<u64,Vec<Mnemonic>>,HashMap<u64,Vec<(Option<u64>,Guard)>>,HashMap<u64,Vec<(Option<u64>,Guard)>>) {
        let mut mnemonics = BTreeMap::new();
        let mut by_source = HashMap::<u64,Vec<(Option<u64>,Guard)>>::new();
        let mut by_destination = HashMap::<u64,Vec<(Option<u64>,Guard)>>::new();

        for v in g.vertices() {
            if let Some(&ControlFlowTarget::Resolved(ref bb)) = g.vertex_label(v) {
                let mut prev_mne = None;

                for mne in &bb.mnemonics {
                    mnemonics.entry(mne.area.start).or_insert(Vec::new()).push(mne.clone());

                    if let Some(prev) = prev_mne {
                        by_source.entry(prev).or_insert(Vec::new()).push((Some(mne.area.start),Guard::always()));
                        by_destination.entry(mne.area.start).or_insert(Vec::new()).push((Some(prev),Guard::always()));
                    }
                    prev_mne = Some(mne.area.start);
                }
            }
        }

        for e in g.edges() {
            let gu = g.edge_label(e).unwrap().clone();
            let src = g.vertex_label(g.source(e));
            let tgt = g.vertex_label(g.target(e));

            match (src,tgt) {
                (Some(&ControlFlowTarget::Resolved(ref src_bb)),Some(&ControlFlowTarget::Resolved(ref tgt_bb))) => {
                    let last = src_bb.mnemonics.last().map_or(src_bb.area.start,|mne| mne.area.start);
                    by_source.entry(last).or_insert(Vec::new()).push((Some(tgt_bb.area.start),gu.clone()));
                    by_destination.entry(tgt_bb.area.start).or_insert(Vec::new()).push((Some(last),gu));
                },
                (Some(&ControlFlowTarget::Resolved(ref src_bb)),Some(&ControlFlowTarget::Unresolved(Rvalue::Constant{ value: ref c,.. }))) => {
                    let last = src_bb.mnemonics.last().map_or(src_bb.area.start,|mne| mne.area.start);
                    by_source.entry(last).or_insert(Vec::new()).push((Some(*c),gu.clone()));
                    by_destination.entry(*c).or_insert(Vec::new()).push((Some(last),gu));
                },
                (Some(&ControlFlowTarget::Unresolved(Rvalue::Constant{ value: ref c,.. })),Some(&ControlFlowTarget::Resolved(ref tgt_bb))) => {
                    by_source.entry(*c).or_insert(Vec::new()).push((Some(tgt_bb.area.start),gu.clone()));
                    by_destination.entry(tgt_bb.area.start).or_insert(Vec::new()).push((Some(*c),gu));
                },
                (Some(&ControlFlowTarget::Resolved(ref src_bb)),Some(&ControlFlowTarget::Unresolved(_))) => {
                    by_source.entry(src_bb.area.start).or_insert(Vec::new()).push((None,gu));
                },
                (Some(&ControlFlowTarget::Unresolved(_)),Some(&ControlFlowTarget::Resolved(ref tgt_bb))) => {
                    by_destination.entry(tgt_bb.area.start).or_insert(Vec::new()).push((None,gu));
                },
                _ => {}
            }
        }

        (mnemonics,by_source,by_destination)
    }

    fn assemble_cflow_graph(mnemonics: BTreeMap<u64,Vec<Mnemonic>>,
                            by_source: HashMap<u64,Vec<(Option<u64>,Guard)>>,
                            by_destination: HashMap<u64,Vec<(Option<u64>,Guard)>>,
                            start: u64) -> ControlFlowGraph
    {
        let mut ret = ControlFlowGraph::new();
        let mut bblock = Vec::<Mnemonic>::new();

        for (_,mnes) in mnemonics.iter() {
            for mne in mnes {
                if bblock.len() > 0 {
                    let last_mne = &bblock.last().unwrap().clone();
                    // if next mnemonics aren't adjacent
                    let mut new_bb = bblock.last().unwrap().area.end != mne.area.start;

					// or any following jumps aren't to adjacent mnemonics
                    new_bb |= by_source.get(&last_mne.area.start).unwrap_or(&Vec::new()).iter().any(|&(ref opt_dest,_)| {
                        opt_dest.is_some() && opt_dest.unwrap() != mne.area.start });

					// or any jumps pointing to the next that aren't from here
                    new_bb |= by_destination.get(&mne.area.start).unwrap_or(&Vec::new()).iter().any(|&(ref opt_src,_)| {
                        opt_src.is_some() && opt_src.unwrap() != last_mne.area.start });

                    // or the entry point does not point here
                    new_bb |= mne.area.start == start;

                    if new_bb {
                        let bb = BasicBlock::from_vec(bblock.clone());

                        bblock.clear();
                        ret.add_vertex(ControlFlowTarget::Resolved(bb));
                    }
                }

                bblock.push(mne.clone());
            }
        }

        // last basic block
        if !bblock.is_empty() {
            ret.add_vertex(ControlFlowTarget::Resolved(BasicBlock::from_vec(bblock)));
        }

        // connect basic blocks
        for (src_off,tgts) in by_source.iter() {
            for &(ref opt_tgt,ref gu) in tgts {
                if opt_tgt.is_some() {

                    let from_bb = ret.vertices().find(|&t| {
                        match ret.vertex_label(t) {
                            Some(&ControlFlowTarget::Resolved(ref bb)) => bb.mnemonics.last().map_or(false,|x| x.area.start == *src_off),
                            _ => false
                        }
                    });
                    let to_bb = ret.vertices().find(|&t| {
                        match ret.vertex_label(t) {
                            Some(&ControlFlowTarget::Resolved(ref bb)) => bb.area.start == opt_tgt.unwrap(),
                            _ => false
                        }
                    });

                    match (from_bb,to_bb) {
                        (Some(from),Some(to)) => { ret.add_edge(gu.clone(),from,to); },
                        (None,Some(to)) => {
                            if let Some(&ControlFlowTarget::Resolved(ref bb)) = ret.vertex_label(to) {
                                if bb.area.start <= *src_off && bb.area.end > *src_off {
                                    continue;
                                }
                            }

                            let vx = ret.add_vertex(ControlFlowTarget::Unresolved(Rvalue::new_u64(*src_off)));
                            ret.add_edge(gu.clone(),vx,to);
                        },
                        (Some(from),None) => {
                            if let Some(&ControlFlowTarget::Resolved(ref bb)) = ret.vertex_label(from) {
                                if bb.area.start <= opt_tgt.unwrap() && bb.area.end > opt_tgt.unwrap() {
                                    continue;
                                }
                            }

                            let vx = ret.add_vertex(ControlFlowTarget::Unresolved(Rvalue::new_u64(opt_tgt.unwrap())));
                            ret.add_edge(gu.clone(),from,vx);
                        },
                        _ => error!("jump from {} to {} doesn't hit any blocks",src_off,opt_tgt.unwrap()),
                    }
                }
            }
        }

        ret
    }

    pub fn disassemble<A: Architecture>(cont: Option<Function>, dec: Rc<Disassembler<A>>, init: A::Configuration, data: LayerIter, start: u64, reg: String) -> Function {
        let name = cont.as_ref().map_or(format!("func_{}",start),|x| x.name.clone());
        let uuid = cont.as_ref().map_or(Uuid::new_v4(),|x| x.uuid.clone());
        let maybe_entry = if let Some(Function{ entry_point: ent, cflow_graph: ref cfg, ..}) = cont {
            if let Some(ref v) = ent {
                match cfg.vertex_label(*v) {
                    Some(&ControlFlowTarget::Resolved(ref bb)) => Some(bb.area.start),
                    _ => None
                }
            } else {
                None
            }
        } else {
            Some(start)
        };
        let (mut mnemonics,mut by_source,mut by_destination) = cont.map_or(
            (BTreeMap::new(),HashMap::new(),HashMap::new()),|x| Self::index_cflow_graph(x.cflow_graph));
        let mut todo = BTreeSet::<u64>::new();

        todo.insert(start);

        while !todo.is_empty() {
            let addr = todo.iter().next().unwrap().clone();
            let maybe_mnes = mnemonics.iter().find(|x| *x.0 >= addr).map(|x| x.1.clone());

            todo.remove(&addr);

            if let Some(mnes) = maybe_mnes {
                if !mnes.is_empty() {
                    let a = mnes.first().unwrap().area.clone();
                    if a.start < addr && a.end > addr {
                        println!("jump inside mnemonic at {}",addr);
                    } else if a.start == addr {
                        // else: already disassembled
                        continue;
                    }
                }
            }

            let mut i = data.seek(addr);
            let maybe_match = dec.next_match(&mut i,addr,init.clone());

            if let Some(match_st) = maybe_match {
                for mne in match_st.mnemonics {
                    println!("{:x}: {} ({:?})",mne.area.start,mne.opcode,match_st.tokens);
                    mnemonics.entry(mne.area.start).or_insert(Vec::new()).push(mne);

                }

                for (origin,tgt,gu) in match_st.jumps {
                    match tgt {
                        Rvalue::Constant{ value: ref c,.. } => {
                            by_source.entry(origin).or_insert(Vec::new()).push((Some(*c),gu.clone()));
                            by_destination.entry(*c).or_insert(Vec::new()).push((Some(origin),gu.clone()));
                            todo.insert(*c);
                        },
                        _ => {
                            by_source.entry(origin).or_insert(Vec::new()).push((None,gu.clone()));
                        }
                    }
                }
            } else {
                println!("failed to match anything at {:x}",addr);
            }
        }

        let cfg = Self::assemble_cflow_graph(mnemonics,by_source,by_destination,start);

        let e = if let Some(addr) = maybe_entry {
            cfg.vertices().find(|&vx| {
                if let Some(&ControlFlowTarget::Resolved(ref bb)) = cfg.vertex_label(vx) {
                    bb.area.start == addr
                } else {
                    false
                }
            })
        } else {
            None
        };

        Function{
            uuid: uuid,
            name: name,
            cflow_graph: cfg,
            entry_point: e,
            region: reg,
        }
    }

    pub fn collect_calls(&self) -> Vec<Rvalue> {
        let mut ret = Vec::new();

        for vx in self.cflow_graph.vertices() {
            if let Some(&ControlFlowTarget::Resolved(ref bb)) = self.cflow_graph.vertex_label(vx) {
                bb.execute(|i| match i {
                    &Statement{ op: Operation::Call(ref t), ..} => ret.push(t.clone()),
                    _ => {}
                });
            }
        }

        ret
    }

    pub fn find_basic_block_at_address(&self,a: u64) -> Option<ControlFlowRef> {
        self.cflow_graph.vertices().find(|&x| {
            match self.cflow_graph.vertex_label(x) {
                Some(&ControlFlowTarget::Resolved(ref bb)) => {
                    bb.area.start <= a && bb.area.end > a
                },
                _ => false
            }
        })
    }

    pub fn postorder(&self) -> Vec<ControlFlowRef> {
        assert!(self.entry_point.is_some());
        TreeIterator::new(self.entry_point.unwrap(),TraversalOrder::Postorder,&self.cflow_graph).
            collect()
    }

    pub fn to_dot(&self) -> String {
        let mut ret = "digraph G {".to_string();

        for v in self.cflow_graph.vertices() {
            match self.cflow_graph.vertex_label(v) {
                Some(&ControlFlowTarget::Resolved(ref bb)) => {
                    ret = format!("{}\n{} [label=<<table border=\"0\"><tr><td>{}:{}</td></tr>",ret,v.0,bb.area.start,bb.area.end);

                    for mne in bb.mnemonics.iter() {
                        ret = format!("{}<tr><td align=\"left\">{}</td></tr>",ret,mne.opcode);
                        for i in mne.instructions.iter() {
                            ret = format!("{}<tr><td align=\"left\">&nbsp;&nbsp;&nbsp;&nbsp;{}</td></tr>",ret,i);
                        }
                    }

                    ret = format!("{}</table>>,shape=record];",ret);
                },
                Some(&ControlFlowTarget::Unresolved(ref c)) => {
                    ret = format!("{}\n{} [label=\"{:?}\",shape=circle];",ret,v.0,c);
                }
                _ => {
                    ret = format!("{}\n{} [label=\"?\",shape=circle];",ret,v.0);
                }
            }
        }

        for e in self.cflow_graph.edges() {
            ret = format!("{}\n{} -> {} [label=\"{}\"];",
                          ret,
                          self.cflow_graph.source(e).0,
                          self.cflow_graph.target(e).0,
                          self.cflow_graph.edge_label(e).unwrap());
        }

        format!("{}\n}}",ret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;
    use graph_algos::{VertexListGraphTrait,EdgeListGraphTrait,AdjacencyMatrixGraphTrait};
    use graph_algos::{GraphTrait,MutableGraphTrait};
    use {
        Guard,
        Mnemonic,
        Bound,
        BasicBlock,
        Rvalue,
        OpaqueLayer,
        State,
        Architecture
    };

    #[derive(Clone)]
    enum TestArchShort {}
    impl Architecture for TestArchShort {
        type Token = u8;
        type Configuration = ();
    }

    #[derive(Clone)]
    enum TestArchWide {}
    impl Architecture for TestArchWide {
        type Token = u16;
        type Configuration = ();
    }

    #[test]
    fn new() {
        let f = Function::new("test".to_string(),"ram".to_string());

        assert_eq!(f.name, "test".to_string());
        assert_eq!(f.cflow_graph.num_vertices(), 0);
        assert_eq!(f.cflow_graph.num_edges(), 0);
        assert_eq!(f.entry_point, None);
    }

    #[test]
    fn index_resolved() {
        let mut cfg = ControlFlowGraph::new();

        let bb0 = BasicBlock::from_vec(vec!(
                Mnemonic::dummy(0..1),
                Mnemonic::dummy(1..2),
                Mnemonic::dummy(2..5),
                Mnemonic::dummy(5..6)));
        let bb1 = BasicBlock::from_vec(vec!(
                Mnemonic::dummy(10..11),
                Mnemonic::dummy(11..12),
                Mnemonic::dummy(12..15),
                Mnemonic::dummy(15..16)));
        let bb2 = BasicBlock::from_vec(vec!(
                Mnemonic::dummy(6..10)));

        let vx0 = cfg.add_vertex(ControlFlowTarget::Resolved(bb0));
        let vx1 = cfg.add_vertex(ControlFlowTarget::Resolved(bb1));
        let vx2 = cfg.add_vertex(ControlFlowTarget::Resolved(bb2));

        cfg.add_edge(Guard::always(),vx0,vx1);
        cfg.add_edge(Guard::always(),vx1,vx1);
        cfg.add_edge(Guard::always(),vx1,vx2);
        cfg.add_edge(Guard::always(),vx2,vx0);

        let (mnes,src,dest) = Function::index_cflow_graph(cfg);

        assert_eq!(mnes.len(),9);
        assert_eq!(src.values().fold(0,|acc,x| acc + x.len()),10);
        assert_eq!(dest.values().fold(0,|acc,x| acc + x.len()),10);

        let cfg_re = Function::assemble_cflow_graph(mnes,src,dest,0);

        assert_eq!(cfg_re.num_vertices(), 3);
        assert_eq!(cfg_re.num_edges(), 4);

        for vx in cfg_re.vertices() {
            if let Some(&ControlFlowTarget::Resolved(ref bb)) = cfg_re.vertex_label(vx) {
                assert!(
                    (bb.area.start == 0 && bb.area.end == 6) ||
                    (bb.area.start == 10 && bb.area.end == 16) ||
                    (bb.area.start == 6 && bb.area.end == 10)
                );
            } else {
                unreachable!();
            }
        }

        for e in cfg_re.edges() {
            if let Some(&ControlFlowTarget::Resolved(ref from)) = cfg_re.vertex_label(cfg_re.source(e)) {
                if let Some(&ControlFlowTarget::Resolved(ref to)) = cfg_re.vertex_label(cfg_re.target(e)) {
                    assert!(
                        (from.area.start == 0 && to.area.start == 10) ||
                        (from.area.start == 10 && to.area.start == 10) ||
                        (from.area.start == 10 && to.area.start == 6) ||
                        (from.area.start == 6 && to.area.start == 0)
                    );
                } else {
                    unreachable!();
                }
            } else {
                unreachable!();
            }
        }
    }

    #[test]
    fn index_unresolved() {
        let mut cfg = ControlFlowGraph::new();

        let bb0 = BasicBlock::from_vec(vec!(
                Mnemonic::dummy(0..1)));
        let bb1 = BasicBlock::from_vec(vec!(
                Mnemonic::dummy(10..11)));

        let vx0 = cfg.add_vertex(ControlFlowTarget::Resolved(bb0));
        let vx1 = cfg.add_vertex(ControlFlowTarget::Resolved(bb1));
        let vx2 = cfg.add_vertex(ControlFlowTarget::Unresolved(Rvalue::new_u32(42)));
        let vx3 = cfg.add_vertex(ControlFlowTarget::Unresolved(Rvalue::new_u32(23)));
        let vx4 = cfg.add_vertex(ControlFlowTarget::Unresolved(Rvalue::Variable{ name: Cow::Borrowed("a"), size: 8, offset: 0, subscript: None }));

        cfg.add_edge(Guard::always(),vx0,vx1);
        cfg.add_edge(Guard::always(),vx2,vx1);
        cfg.add_edge(Guard::always(),vx3,vx0);
        cfg.add_edge(Guard::always(),vx4,vx3);

        let (mnes,src,dest) = Function::index_cflow_graph(cfg);

        assert_eq!(mnes.len(),2);
        assert_eq!(src.values().fold(0,|acc,x| acc + x.len()),3);
        assert_eq!(dest.values().fold(0,|acc,x| acc + x.len()),3);

        let cfg_re = Function::assemble_cflow_graph(mnes,src,dest,0);

        assert_eq!(cfg_re.num_vertices(), 4);
        assert_eq!(cfg_re.num_edges(), 3);

        for vx in cfg_re.vertices() {
            match cfg_re.vertex_label(vx) {
                Some(&ControlFlowTarget::Resolved(ref bb)) => {
                    assert!(
                        (bb.area.start == 0 && bb.area.end == 1) ||
                        (bb.area.start == 10 && bb.area.end == 11)
                    );
                },
                Some(&ControlFlowTarget::Unresolved(Rvalue::Constant{ value: ref c, size: 64 })) => {
                    assert!(*c == 42 || *c == 23);
                },
                _ => { unreachable!(); }
            }
        }
    }

    #[test]
    fn add_single() {
        let main = new_disassembler!(TestArchShort =>
            [ 0 ] = |st: &mut State<TestArchShort>| {
                st.mnemonic(1,"A","",vec!(),&|_| {});
                true
            }
		);
        let data = OpaqueLayer::wrap(vec!(0));
        let func = Function::disassemble(None,main,(),data.iter(),0,"ram".to_string());

        assert_eq!(func.cflow_graph.num_vertices(), 1);
        assert_eq!(func.cflow_graph.num_edges(), 0);

        if let Some(vx) = func.cflow_graph.vertices().next() {
            if let Some(&ControlFlowTarget::Resolved(ref bb)) = func.cflow_graph.vertex_label(vx) {
                assert_eq!(bb.mnemonics.len(), 1);
                assert_eq!(bb.mnemonics[0].opcode, "A".to_string());
                assert_eq!(bb.mnemonics[0].area, Bound::new(0,1));
                assert_eq!(bb.area, Bound::new(0,1));
            } else {
                unreachable!();
            }
         } else {
            unreachable!();
        }

        assert_eq!(func.entry_point, func.cflow_graph.vertices().next());
        assert_eq!(func.name, "func_0".to_string());
    }

    #[test]
    fn continuous() {
        let main = new_disassembler!(TestArchShort =>
            [ 0 ] = |st: &mut State<TestArchShort>| {
                let next = st.address;
                st.mnemonic(1,"test0","",vec!(),&|_| {});
                st.jump(Rvalue::new_u64(next + 1),Guard::always());
                true
            },
            [ 1 ] = |st: &mut State<TestArchShort>| {
                let next = st.address;
                st.mnemonic(1,"test1","",vec!(),&|_| {});
                st.jump(Rvalue::new_u64(next + 1),Guard::always());
                true
            },
            [ 2 ] = |st: &mut State<TestArchShort>| {
                let next = st.address;
                st.mnemonic(1,"test2","",vec!(),&|_| {});
                st.jump(Rvalue::new_u64(next + 1),Guard::always());
                true
            },
            [ 3 ] = |st: &mut State<TestArchShort>| {
                let next = st.address;
                st.mnemonic(1,"test3","",vec!(),&|_| {});
                st.jump(Rvalue::new_u64(next + 1),Guard::always());
                true
            },
            [ 4 ] = |st: &mut State<TestArchShort>| {
                let next = st.address;
                st.mnemonic(1,"test4","",vec!(),&|_| {});
                st.jump(Rvalue::new_u64(next + 1),Guard::always());
                true
            },
            [ 5 ] = |st: &mut State<TestArchShort>| {
                let next = st.address;
                st.mnemonic(1,"test5","",vec!(),&|_| {});
                st.jump(Rvalue::new_u64(next + 1),Guard::always());
                true
            }
        );

        let data = OpaqueLayer::wrap(vec!(0,1,2,3,4,5));
        let func = Function::disassemble(None,main,(),data.iter(),0,"ram".to_string());

        assert_eq!(func.cflow_graph.num_vertices(), 2);
        assert_eq!(func.cflow_graph.num_edges(), 1);

        let mut bb_vx = None;
        let mut ures_vx = None;

        for vx in func.cflow_graph.vertices() {
            if let Some(&ControlFlowTarget::Resolved(ref bb)) = func.cflow_graph.vertex_label(vx) {
                assert_eq!(bb.mnemonics.len(), 6);
                assert_eq!(bb.mnemonics[0].opcode, "test0".to_string());
                assert_eq!(bb.mnemonics[0].area, Bound::new(0,1));
                assert_eq!(bb.mnemonics[1].opcode, "test1".to_string());
                assert_eq!(bb.mnemonics[1].area, Bound::new(1,2));
                assert_eq!(bb.mnemonics[2].opcode, "test2".to_string());
                assert_eq!(bb.mnemonics[2].area, Bound::new(2,3));
                assert_eq!(bb.mnemonics[3].opcode, "test3".to_string());
                assert_eq!(bb.mnemonics[3].area, Bound::new(3,4));
                assert_eq!(bb.mnemonics[4].opcode, "test4".to_string());
                assert_eq!(bb.mnemonics[4].area, Bound::new(4,5));
                assert_eq!(bb.mnemonics[5].opcode, "test5".to_string());
                assert_eq!(bb.mnemonics[5].area, Bound::new(5,6));
                assert_eq!(bb.area, Bound::new(0,6));
                bb_vx = Some(vx);
            } else if let Some(&ControlFlowTarget::Unresolved(Rvalue::Constant{ value: c, size: 64 })) = func.cflow_graph.vertex_label(vx) {
                assert_eq!(c, 6);
                ures_vx = Some(vx);
            } else {
                unreachable!();
            }
        }

        assert!(ures_vx.is_some() && bb_vx.is_some());
        assert_eq!(func.entry_point, bb_vx);
        assert_eq!(func.name, "func_0".to_string());
        assert!(func.cflow_graph.edge(bb_vx.unwrap(),ures_vx.unwrap()).is_some());
    }

    #[test]
    fn branch() {
        let main = new_disassembler!(TestArchShort =>
            [ 0 ] = |st: &mut State<TestArchShort>| {
                st.mnemonic(1,"test0","",vec!(),&|_| {});
                st.jump(Rvalue::new_u32(1),Guard::always());
                st.jump(Rvalue::new_u32(2),Guard::always());
                true
            },
            [ 1 ] = |st: &mut State<TestArchShort>| {
                st.mnemonic(1,"test1","",vec!(),&|_| {});
                st.jump(Rvalue::new_u32(3),Guard::always());
                true
            },
            [ 2 ] = |st: &mut State<TestArchShort>| {
                st.mnemonic(1,"test2","",vec!(),&|_| {});
                st.jump(Rvalue::new_u32(1),Guard::always());
                true
            }
        );

        let data = OpaqueLayer::wrap(vec!(0,1,2));
        let func = Function::disassemble(None,main,(),data.iter(),0,"ram".to_string());

        assert_eq!(func.cflow_graph.num_vertices(), 4);
        assert_eq!(func.cflow_graph.num_edges(), 4);

        let mut bb0_vx = None;
        let mut bb1_vx = None;
        let mut bb2_vx = None;
        let mut ures_vx = None;

        for vx in func.cflow_graph.vertices() {
            if let Some(&ControlFlowTarget::Resolved(ref bb)) = func.cflow_graph.vertex_label(vx) {
                if bb.area.start == 0 {
                    assert_eq!(bb.mnemonics.len(), 1);
                    assert_eq!(bb.mnemonics[0].opcode, "test0".to_string());
                    assert_eq!(bb.mnemonics[0].area, Bound::new(0,1));
                    assert_eq!(bb.area, Bound::new(0,1));
                    bb0_vx = Some(vx);
                } else if bb.area.start == 1 {
                    assert_eq!(bb.mnemonics.len(), 1);
                    assert_eq!(bb.mnemonics[0].opcode, "test1".to_string());
                    assert_eq!(bb.mnemonics[0].area, Bound::new(1,2));
                    assert_eq!(bb.area, Bound::new(1,2));
                    bb1_vx = Some(vx);
                } else if bb.area.start == 2 {
                    assert_eq!(bb.mnemonics.len(), 1);
                    assert_eq!(bb.mnemonics[0].opcode, "test2".to_string());
                    assert_eq!(bb.mnemonics[0].area, Bound::new(2,3));
                    assert_eq!(bb.area, Bound::new(2,3));
                    bb2_vx = Some(vx);
                } else {
                    unreachable!();
                }
            } else if let Some(&ControlFlowTarget::Unresolved(Rvalue::Constant{ value: c, size: 64 })) = func.cflow_graph.vertex_label(vx) {
                assert_eq!(c, 3);
                ures_vx = Some(vx);
            } else {
                unreachable!();
            }
        }

        assert!(ures_vx.is_some() && bb0_vx.is_some() && bb1_vx.is_some() && bb2_vx.is_some());
        assert_eq!(func.entry_point, bb0_vx);
        assert_eq!(func.name, "func_0".to_string());
        assert!(func.cflow_graph.edge(bb0_vx.unwrap(),bb1_vx.unwrap()).is_some());
        assert!(func.cflow_graph.edge(bb0_vx.unwrap(),bb2_vx.unwrap()).is_some());
        assert!(func.cflow_graph.edge(bb1_vx.unwrap(),ures_vx.unwrap()).is_some());
        assert!(func.cflow_graph.edge(bb2_vx.unwrap(),bb1_vx.unwrap()).is_some());
    }

    #[test]
    fn function_loop() {
      let main = new_disassembler!(TestArchShort =>
            [ 0 ] = |st: &mut State<TestArchShort>| {
                st.mnemonic(1,"test0","",vec!(),&|_| {});
                st.jump(Rvalue::new_u32(1),Guard::always());
                true
            },
            [ 1 ] = |st: &mut State<TestArchShort>| {
                st.mnemonic(1,"test1","",vec!(),&|_| {});
                st.jump(Rvalue::new_u32(2),Guard::always());
                true
            },
            [ 2 ] = |st: &mut State<TestArchShort>| {
                st.mnemonic(1,"test2","",vec!(),&|_| {});
                st.jump(Rvalue::new_u32(0),Guard::always());
                true
            }
        );

        let data = OpaqueLayer::wrap(vec!(0,1,2));
        let func = Function::disassemble(None,main,(),data.iter(),0,"ram".to_string());

        assert_eq!(func.cflow_graph.num_vertices(), 1);
        assert_eq!(func.cflow_graph.num_edges(), 1);

        let vx = func.cflow_graph.vertices().next().unwrap();
        if let Some(&ControlFlowTarget::Resolved(ref bb)) = func.cflow_graph.vertex_label(vx) {
            if bb.area.start == 0 {
                assert_eq!(bb.mnemonics.len(), 3);
                assert_eq!(bb.mnemonics[0].opcode, "test0".to_string());
                assert_eq!(bb.mnemonics[0].area, Bound::new(0,1));
                assert_eq!(bb.mnemonics[1].opcode, "test1".to_string());
                assert_eq!(bb.mnemonics[1].area, Bound::new(1,2));
                assert_eq!(bb.mnemonics[2].opcode, "test2".to_string());
                assert_eq!(bb.mnemonics[2].area, Bound::new(2,3));
                assert_eq!(bb.area, Bound::new(0,3));
            } else {
                unreachable!();
            }
        }

        assert_eq!(func.name, "func_0".to_string());
        assert_eq!(func.entry_point,Some(vx));
        assert!(func.cflow_graph.edge(vx,vx).is_some());
    }

    #[test]
    fn empty() {
        let main = new_disassembler!(TestArchShort =>
            [ 0 ] = |st: &mut State<TestArchShort>| {
                st.mnemonic(1,"test0","",vec!(),&|_| {});
                st.jump(Rvalue::new_u32(1),Guard::always());
                true
            },
            [ 1 ] = |st: &mut State<TestArchShort>| {
                st.mnemonic(1,"test1","",vec!(),&|_| {});
                st.jump(Rvalue::new_u32(2),Guard::always());
                true
            },
            [ 2 ] = |st: &mut State<TestArchShort>| {
                st.mnemonic(1,"test2","",vec!(),&|_| {});
                st.jump(Rvalue::new_u32(0),Guard::always());
                true
            }
        );

        let data = OpaqueLayer::wrap(vec!());
        let func = Function::disassemble(None,main,(),data.iter(),0,"ram".to_string());

        assert_eq!(func.cflow_graph.num_vertices(), 0);
        assert_eq!(func.cflow_graph.num_edges(), 0);
        assert_eq!(func.name, "func_0".to_string());
        assert_eq!(func.entry_point,None);
    }

    #[test]
    fn entry_split() {
        let bb = BasicBlock::from_vec(vec!(Mnemonic::dummy(0..1),Mnemonic::dummy(1..2)));
        let mut fun = Function::new("test_func".to_string(),"ram".to_string());
        let vx0 = fun.cflow_graph.add_vertex(ControlFlowTarget::Resolved(bb));
        let vx1 = fun.cflow_graph.add_vertex(ControlFlowTarget::Unresolved(Rvalue::new_u32(2)));

        fun.entry_point = Some(vx0);
        fun.cflow_graph.add_edge(Guard::always(),vx0,vx1);

        let main = new_disassembler!(TestArchShort =>
            [ 0 ] = |st: &mut State<TestArchShort>| {
                st.mnemonic(1,"test0","",vec!(),&|_| {});
                st.jump(Rvalue::new_u32(1),Guard::always());
                true
            },
            [ 1 ] = |st: &mut State<TestArchShort>| {
                st.mnemonic(1,"test1","",vec!(),&|_| {});
                st.jump(Rvalue::new_u32(2),Guard::always());
                true
            },
            [ 2 ] = |st: &mut State<TestArchShort>| {
                st.mnemonic(1,"test2","",vec!(),&|_| {});
                st.jump(Rvalue::new_u32(1),Guard::always());
                true
            }
        );

        let data = OpaqueLayer::wrap(vec!(0,1,2));
        let func = Function::disassemble(Some(fun),main,(),data.iter(),2,"ram".to_string());

        assert_eq!(func.cflow_graph.num_vertices(), 3);
        assert_eq!(func.cflow_graph.num_edges(), 3);
        assert_eq!(func.name, "test_func".to_string());

        let mut bb0_vx = None;
        let mut bb1_vx = None;
        let mut bb2_vx = None;

        for vx in func.cflow_graph.vertices() {
            if let Some(&ControlFlowTarget::Resolved(ref bb)) = func.cflow_graph.vertex_label(vx) {
                if bb.area.start == 0 {
                    assert_eq!(bb.mnemonics.len(), 1);
                    assert_eq!(bb.mnemonics[0].opcode, "dummy".to_string());
                    assert_eq!(bb.mnemonics[0].area, Bound::new(0,1));
                    assert_eq!(bb.area, Bound::new(0,1));
                    bb0_vx = Some(vx);
                } else if bb.area.start == 1 {
                    assert_eq!(bb.mnemonics.len(), 1);
                    assert_eq!(bb.mnemonics[0].opcode, "dummy".to_string());
                    assert_eq!(bb.mnemonics[0].area, Bound::new(1,2));
                    assert_eq!(bb.area, Bound::new(1,2));
                    bb1_vx = Some(vx);
                } else if bb.area.start == 2 {
                    assert_eq!(bb.mnemonics.len(), 1);
                    assert_eq!(bb.mnemonics[0].opcode, "test2".to_string());
                    assert_eq!(bb.mnemonics[0].area, Bound::new(2,3));
                    assert_eq!(bb.area, Bound::new(2,3));
                    bb2_vx = Some(vx);
                } else {
                    unreachable!();
                }
            } else {
                unreachable!();
            }
        }

        assert!(bb0_vx.is_some() && bb1_vx.is_some() && bb2_vx.is_some());
        assert_eq!(func.entry_point, bb0_vx);
        assert!(func.cflow_graph.edge(bb0_vx.unwrap(),bb1_vx.unwrap()).is_some());
        assert!(func.cflow_graph.edge(bb1_vx.unwrap(),bb2_vx.unwrap()).is_some());
        assert!(func.cflow_graph.edge(bb2_vx.unwrap(),bb1_vx.unwrap()).is_some());
    }

    #[test]
    fn wide_token() {
        let def = OpaqueLayer::wrap(vec!(0x11,0x22,0x33,0x44,0x55,0x44));
        let dec = new_disassembler!(TestArchWide =>
            [0x2211] = |s: &mut State<TestArchWide>|
            {
                let a = s.address;
                s.mnemonic(2,"A","",vec!(),&|_| {});
                s.jump(Rvalue::new_u64(a + 2),Guard::always());
                true
            },

            [0x4433] = |s: &mut State<TestArchWide>|
            {
                let a = s.address;
                s.mnemonic(2,"B","",vec!(),&|_| {});
                s.jump(Rvalue::new_u64(a + 2),Guard::always());
                s.jump(Rvalue::new_u64(a + 4),Guard::always());
                true
            },

            [0x4455] = |s: &mut State<TestArchWide>|
            {
                s.mnemonic(2, "C","",vec!(),&|_| {});
                true
            }
        );

        let func = Function::disassemble(None,dec,(),def.iter(),0,"ram".to_string());

        assert_eq!(func.cflow_graph.num_vertices(), 3);
        assert_eq!(func.cflow_graph.num_edges(), 2);

        let mut bb0_vx = None;
        let mut bb1_vx = None;

        for vx in func.cflow_graph.vertices() {
            match func.cflow_graph.vertex_label(vx) {
                Some(&ControlFlowTarget::Resolved(ref bb)) => {
                    if bb.area.start == 0 {
                        assert_eq!(bb.mnemonics.len(), 2);
                        assert_eq!(bb.area, Bound::new(0,4));
                        bb0_vx = Some(vx);
                    } else if bb.area.start == 4 {
                        assert_eq!(bb.mnemonics.len(), 1);
                        assert_eq!(bb.area, Bound::new(4,6));
                        bb1_vx = Some(vx);
                    } else {
                        unreachable!();
                    }
                },
                Some(&ControlFlowTarget::Unresolved(Rvalue::Constant{ value: 6, size: 64 })) => {},
                _ => unreachable!()
            }
        }

        assert!(bb0_vx.is_some() && bb1_vx.is_some());
        assert_eq!(func.entry_point, bb0_vx);
    }

    #[test]
    fn issue_51_treat_entry_point_as_incoming_edge() {
        let main = new_disassembler!(TestArchShort =>
            [ 0 ] = |st: &mut State<TestArchShort>| {
                st.mnemonic(1,"test0","",vec!(),&|_| {});
                st.jump(Rvalue::new_u32(1),Guard::always());
                true
            },
            [ 1 ] = |st: &mut State<TestArchShort>| {
                st.mnemonic(1,"test1","",vec!(),&|_| {});
                st.jump(Rvalue::new_u32(2),Guard::always());
                true
            },
            [ 2 ] = |st: &mut State<TestArchShort>| {
                st.mnemonic(1,"test2","",vec!(),&|_| {});
                st.jump(Rvalue::new_u32(0),Guard::always());
                true
            }
        );

        let data = OpaqueLayer::wrap(vec!(0,1,2));
        let func = Function::disassemble(None,main,(),data.iter(),1,"ram".to_string());

        assert_eq!(func.cflow_graph.num_vertices(), 2);
        assert_eq!(func.cflow_graph.num_edges(), 2);

        let mut bb0_vx = None;
        let mut bb1_vx = None;

        for vx in func.cflow_graph.vertices() {
            if let Some(&ControlFlowTarget::Resolved(ref bb)) = func.cflow_graph.vertex_label(vx) {
                if bb.area.start == 0 {
                    assert_eq!(bb.mnemonics.len(), 1);
                    assert_eq!(bb.area, Bound::new(0,1));
                    bb0_vx = Some(vx);
                } else if bb.area.start == 1 {
                    assert_eq!(bb.mnemonics.len(), 2);
                    assert_eq!(bb.area, Bound::new(1,3));
                    bb1_vx = Some(vx);
                } else {
                    unreachable!();
                }
            } else {
                unreachable!();
            }
        }

        assert!(bb0_vx.is_some() && bb1_vx.is_some());
        assert_eq!(func.entry_point, bb1_vx);
        assert!(func.cflow_graph.edge(bb0_vx.unwrap(),bb1_vx.unwrap()).is_some());
        assert!(func.cflow_graph.edge(bb1_vx.unwrap(),bb0_vx.unwrap()).is_some());
    }
}
