/*
 * Panopticon - A libre disassembler
 * Copyright (C) 2015  Panopticon authors
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

use std::io::Read;
use std::fs::File;
use std::path::Path;

use {
    Program,
    CallTarget,
    Project,
    Layer,
    Region,
    Bound,
    Target,
    Rvalue,
};

use graph_algos::{
    MutableGraphTrait,
    GraphTrait
};
use uuid::Uuid;

use std::borrow::Cow;

#[derive(Debug)]
pub struct Error {
    pub msg: Cow<'static,str>
}

impl Error {
    pub fn new(s: &'static str) -> Error {
        Error{ msg: Cow::Borrowed(s) }
    }

    pub fn new_owned(s: String) -> Error {
        Error{ msg: Cow::Owned(s) }
    }
}

pub fn load(p: &Path) -> Result<Project,Error> {
    let mut fd = File::open(p).ok().unwrap();
    let mut buf = vec![0u8; 2];
    if let Err(_) = fd.read(&mut buf) {
        return Err(Error::new("Failed to read load address"));
    }
    let addr = ((buf[1] as u64) << 8) + buf[0] as u64;
    let size = fd.metadata().ok().unwrap().len() - 2;

    let mut reg = Region::undefined("base".to_string(), 0x10000);

    let mut buf = vec![0u8; size as usize];
    if let Err(_) = fd.read(&mut buf) {
        return Err(Error::new("Failed to read segment"));
    }
    reg.cover(Bound::new(addr, addr + size), Layer::wrap(buf));

    let name = p.file_name()
        .map(|x| x.to_string_lossy().to_string())
        .unwrap_or("(encoding error)".to_string());

    let mut prog = Program::new("prog0", Target::Mos6502);
    let mut proj = Project::new(name.clone(),reg);

    prog.call_graph.add_vertex(CallTarget::Todo(Rvalue::new_u16(addr),Some(name),Uuid::new_v4()));
    proj.comments.insert(("base".to_string(),addr),"main".to_string());

    for &(name,ref off,cmnt) in Target::Mos6502.interrupt_vec().iter() {
        let uu =  Uuid::new_v4();
		prog.call_graph.add_vertex(CallTarget::Todo(off.clone(), Some(name.to_string()),uu));
	}

    proj.code.push(prog);

    Ok(proj)
}
