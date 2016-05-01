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

use std::borrow::Cow;

use {
    Lvalue,
    Rvalue,
    State,
    CodeGen,
    Guard,
};
use amd64::*;

pub fn decode_m(sm: &mut State<Amd64>) -> Option<Rvalue> {
    sm.clone().configuration.rm.map(|x| x.clone().into())
}

pub fn decode_d(sm: &mut State<Amd64>) -> Option<Rvalue> {
    if let Some(Rvalue::Constant{ ref value,.. }) = sm.configuration.imm {
        Some(if *value <= 0xffffffff {
            Rvalue::new_u64(*value >> 16 | ((*value & 0xffff) << 16))
        } else {
            Rvalue::new_u64(*value >> 32 | ((*value & 0xffffffff) << 32))
        })
    } else {
        None
    }
}

pub fn decode_imm(sm: &mut State<Amd64>) -> Option<Rvalue> {
    sm.configuration.imm.clone()
}

pub fn decode_moffs(sm: &mut State<Amd64>) -> Option<Rvalue> {
    sm.configuration.moffs.clone()
}

pub fn decode_rm1(sm: &mut State<Amd64>) -> Option<Rvalue> {
    sm.clone().configuration.rm.map(|x| x.clone().into())
}

pub fn decode_i(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue)> {
    if let Some(ref imm) = sm.configuration.imm {
        match &sm.configuration.operand_size {
            &OperandSize::Eight => Some((AH.clone().into(),imm.clone())),
            &OperandSize::Sixteen => Some((AX.clone().into(),imm.clone())),
            &OperandSize::ThirtyTwo => Some((EAX.clone().into(),imm.clone())),
            &OperandSize::SixtyFour => Some((RAX.clone().into(),imm.clone())),
            &OperandSize::HundredTwentyEight => None,
        }
    } else {
        None
    }
}

pub fn decode_rm(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue)> {
    if let (&Some(ref reg),&Some(ref rm)) = (&sm.configuration.reg,&sm.configuration.rm) {
        Some((reg.clone().into(),rm.clone().into()))
    } else {
        None
    }
}

pub fn decode_mr(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue)> {
    decode_rm(sm).map(|(a,b)| (b,a))
}

pub fn decode_fd(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue)> {
    sm.clone().configuration.moffs.map(|moffs| (
        select_reg(&sm.configuration.operand_size,0,sm.configuration.rex).clone().into(),
        moffs.clone().into()
    ))
}

pub fn decode_td(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue)> {
    decode_fd(sm).map(|(a,b)| (b,a))
}

pub fn decode_msreg(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue)> {
    decode_sregm(sm).map(|(a,b)| (b,a))
}

pub fn decode_sregm(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue)> {
    if let (&Some(ref reg),&Some(ref rm)) = (&sm.configuration.reg,&sm.configuration.rm) {
        if *reg == *AX || *reg == *EAX  {
            Some((ES.clone().into(),rm.clone().into()))
        } else if *reg == *CX || *reg == *ECX  {
            Some((CS.clone().into(),rm.clone().into()))
        } else if *reg == *DX || *reg == *EDX  {
            Some((SS.clone().into(),rm.clone().into()))
        } else if *reg == *BX || *reg == *EBX  {
            Some((DS.clone().into(),rm.clone().into()))
        } else if *reg == *SP || *reg == *ESP  {
            Some((FS.clone().into(),rm.clone().into()))
        } else if *reg == *BP || *reg == *EBP  {
            Some((GS.clone().into(),rm.clone().into()))
        } else {
            None
        }
    } else {
        None
    }
}

pub fn decode_dbgrm(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue)> {
    if let (&Some(ref reg),&Some(ref rm)) = (&sm.configuration.reg,&sm.configuration.rm) {
        if *reg == *RAX || *reg == *EAX  {
            Some((DR0.clone().into(),rm.clone().into()))
        } else if *reg == *RCX || *reg == *ECX  {
            Some((DR1.clone().into(),rm.clone().into()))
        } else if *reg == *RDX || *reg == *EDX  {
            Some((DR2.clone().into(),rm.clone().into()))
        } else if *reg == *RBX || *reg == *EBX  {
            Some((DR3.clone().into(),rm.clone().into()))
        } else if *reg == *RSP || *reg == *ESP  {
            Some((DR4.clone().into(),rm.clone().into()))
        } else if *reg == *RBP || *reg == *EBP  {
            Some((DR5.clone().into(),rm.clone().into()))
        } else if *reg == *RDI || *reg == *EDI  {
            Some((DR7.clone().into(),rm.clone().into()))
        } else if *reg == *RSI || *reg == *ESI  {
            Some((DR6.clone().into(),rm.clone().into()))
        } else {
            None
        }
    } else {
        None
    }
}

pub fn decode_rmdbg(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue)> {
    decode_dbgrm(sm).map(|(a,b)| (b,a))
}

pub fn decode_ctrlrm(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue)> {
    if let (&Some(ref reg),&Some(ref rm)) = (&sm.configuration.reg,&sm.configuration.rm) {
        if *reg == *RAX || *reg == *EAX  {
            Some((CR0.clone().into(),rm.clone().into()))
        } else if *reg == *RDX || *reg == *EDX  {
            Some((CR2.clone().into(),rm.clone().into()))
        } else if *reg == *RBX || *reg == *EBX  {
            Some((CR3.clone().into(),rm.clone().into()))
        } else if *reg == *RSP || *reg == *ESP  {
            Some((CR4.clone().into(),rm.clone().into()))
        } else if *reg == *R8 || *reg == *R9W  {
            Some((CR8.clone().into(),rm.clone().into()))
        } else {
            None
        }
    } else {
        None
    }
}

pub fn decode_rmctrl(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue)> {
    decode_ctrlrm(sm).map(|(a,b)| (b,a))
}

pub fn decode_mi(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue)> {
    if let (&Some(ref rm),&Some(ref imm)) = (&sm.configuration.rm,&sm.configuration.imm) {
        Some((rm.clone().into(),imm.clone()))
    } else {
        None
    }
}

pub fn decode_m1(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue)> {
    if let &Some(ref rm) = &sm.configuration.rm {
        Some((rm.clone().into(),Rvalue::new_u64(1)))
    } else {
        None
    }
}

pub fn decode_mc(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue)> {
    if let &Some(ref rm) = &sm.configuration.rm {
        Some((rm.clone().into(),CF.clone().into()))
    } else {
        None
    }
}

pub fn decode_ii(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue)> {
    if let &Some(Rvalue::Constant{ ref value,.. }) = &sm.configuration.imm {
        Some((Rvalue::new_u64(*value >> 8),Rvalue::new_u64(*value & 0xff)))
    } else {
        None
    }
}

pub fn decode_rvm(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue,Rvalue)> {
    if let (&Some(ref reg),&Some(ref rm),&Some(ref v)) = (&sm.configuration.reg,&sm.configuration.rm,&sm.configuration.vvvv) {
        Some((reg.clone().into(),rm.clone().into(),v.clone().into()))
    } else {
        None
    }
}

pub fn decode_rmv(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue,Rvalue)> {
    decode_rvm(sm).map(|(a,b,c)| (a,c,b))
}

pub fn decode_rm0(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue,Rvalue)> {
    if let (&Some(ref reg),&Some(ref rm)) = (&sm.configuration.reg,&sm.configuration.rm) {
        Some((reg.clone().into(),rm.clone().into(),Rvalue::new_u64(0)))
    } else {
        None
    }
}

pub fn decode_rmi(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue,Rvalue)> {
    if let (&Some(ref reg),&Some(ref rm),&Some(ref imm)) = (&sm.configuration.reg,&sm.configuration.rm,&sm.configuration.imm) {
        Some((reg.clone().into(),rm.clone().into(),imm.clone()))
    } else {
        None
    }
}

pub fn decode_mri(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue,Rvalue)> {
    decode_rmi(sm).map(|(a,b,c)| (b,a,c))
}

pub fn decode_mvr(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue,Rvalue)> {
    decode_rvm(sm).map(|(a,b,c)| (c,b,a))
}

pub fn decode_vmi(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue,Rvalue)> {
    if let (&Some(ref v),&Some(ref rm),&Some(ref imm)) = (&sm.configuration.vvvv,&sm.configuration.rm,&sm.configuration.imm) {
        Some((v.clone().into(),rm.clone().into(),imm.clone()))
    } else {
        None
    }
}

pub fn decode_vrmi(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue,Rvalue,Rvalue)> {
    if let (&Some(ref v),&Some(ref r),&Some(ref rm),&Some(ref imm)) = (&sm.configuration.vvvv,&sm.configuration.reg,&sm.configuration.rm,&sm.configuration.imm) {
        Some((v.clone().into(),r.clone().into(),rm.clone().into(),imm.clone()))
    } else {
        None
    }
}

pub fn decode_rvmr(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue,Rvalue,Rvalue)> {
    if let (&Some(ref v),&Some(ref r),&Some(ref rm),&Some(ref imm)) = (&sm.configuration.vvvv,&sm.configuration.reg,&sm.configuration.rm,&sm.configuration.imm) {
        Some((r.clone().into(),v.clone().into(),rm.clone().into(),imm.clone()))
    } else {
        None
    }
}

pub fn decode_rvmi(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue,Rvalue,Rvalue)> {
    if let (&Some(ref v),&Some(ref r),&Some(ref rm),&Some(ref imm)) = (&sm.configuration.vvvv,&sm.configuration.reg,&sm.configuration.rm,&sm.configuration.imm) {
        Some((r.clone().into(),v.clone().into(),rm.clone().into(),imm.clone()))
    } else {
        None
    }
}

pub fn decode_sti(sm: &mut State<Amd64>) -> Option<Rvalue> {
    if let Some(tok) = sm.tokens.last() {
        Some(match tok & 15 {
            0x0 => ST0.clone().into(),
            0x1 => ST1.clone().into(),
            0x2 => ST2.clone().into(),
            0x3 => ST3.clone().into(),
            0x4 => ST4.clone().into(),
            0x5 => ST5.clone().into(),
            0x6 => ST6.clone().into(),
            0x7 => ST7.clone().into(),
            0x8 => ST0.clone().into(),
            0x9 => ST1.clone().into(),
            0xa => ST2.clone().into(),
            0xb => ST3.clone().into(),
            0xc => ST4.clone().into(),
            0xd => ST5.clone().into(),
            0xe => ST6.clone().into(),
            0xf => ST7.clone().into(),
            _ => unreachable!()
        })
    } else {
        None
    }
}

pub fn decode_sti0(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue)> {
    if let Some(st) = decode_sti(sm) {
        Some((st,ST0.clone().into()))
    } else {
        None
    }
}

pub fn decode_st0i(sm: &mut State<Amd64>) -> Option<(Rvalue,Rvalue)> {
    if let Some(st) = decode_sti(sm) {
        Some((ST0.clone().into(),st))
    } else {
        None
    }
}

pub fn decode_m64fp(sm: &mut State<Amd64>) -> Option<Rvalue> {
    sm.configuration.rm.as_ref().map(|x| x.clone().into())
}

pub fn decode_m32fp(sm: &mut State<Amd64>) -> Option<Rvalue> {
    sm.configuration.rm.as_ref().map(|x| x.clone().into())
}

pub fn decode_m80fp(sm: &mut State<Amd64>) -> Option<Rvalue> {
    sm.configuration.rm.as_ref().map(|x| x.clone().into())
}

pub fn decode_m16int(sm: &mut State<Amd64>) -> Option<Rvalue> {
    sm.configuration.rm.as_ref().map(|x| x.clone().into())
}

pub fn decode_m32int(sm: &mut State<Amd64>) -> Option<Rvalue> {
    sm.configuration.rm.as_ref().map(|x| x.clone().into())
}

pub fn decode_m64int(sm: &mut State<Amd64>) -> Option<Rvalue> {
    sm.configuration.rm.as_ref().map(|x| x.clone().into())
}

pub fn decode_m2byte(sm: &mut State<Amd64>) -> Option<Rvalue> {
    sm.configuration.rm.as_ref().map(|x| x.clone().into())
}

pub fn decode_m14_28byte(sm: &mut State<Amd64>) -> Option<Rvalue> {
    sm.configuration.rm.as_ref().map(|x| x.clone().into())
}

pub fn decode_m80bcd(sm: &mut State<Amd64>) -> Option<Rvalue> {
    sm.configuration.rm.as_ref().map(|x| x.clone().into())
}

pub fn decode_m80dec(sm: &mut State<Amd64>) -> Option<Rvalue> {
    sm.configuration.rm.as_ref().map(|x| x.clone().into())
}

pub fn decode_m94_108byte(sm: &mut State<Amd64>) -> Option<Rvalue> {
    sm.configuration.rm.as_ref().map(|x| x.clone().into())
}

pub fn decode_reg8(r_reg: u64,rex: bool) -> Lvalue {
    match r_reg {
        0 => AL.clone(),
        1 => CL.clone(),
        2 => DL.clone(),
        3 => BL.clone(),
        4 => if rex { SPL.clone() } else { AH.clone() },
        5 => if rex { BPL.clone() } else { CH.clone() },
        6 => if rex { SIL.clone() } else { DH.clone() },
        7 => if rex { DIL.clone() } else { BH.clone() },
        8 => R8L.clone(),
        9 => R9L.clone(),
        10 => R10L.clone(),
        11 => R11L.clone(),
        12 => R12L.clone(),
        13 => R13L.clone(),
        14 => R14L.clone(),
        15 => R15L.clone(),
        _ => unreachable!()
    }
}

pub fn decode_reg16(r_reg: u64) -> Lvalue {
    match r_reg {
        0 => AX.clone(),
        1 => CX.clone(),
        2 => DX.clone(),
        3 => BX.clone(),
        4 => SP.clone(),
        5 => BP.clone(),
        6 => SI.clone(),
        7 => DI.clone(),
        8 => R8W.clone(),
        9 => R9W.clone(),
        10 => R10W.clone(),
        11 => R11W.clone(),
        12 => R12W.clone(),
        13 => R13W.clone(),
        14 => R14W.clone(),
        15 => R15W.clone(),
        _ => unreachable!()
    }
}

pub fn decode_reg32(r_reg: u64) -> Lvalue {
    match r_reg {
        0 => EAX.clone(),
        1 => ECX.clone(),
        2 => EDX.clone(),
        3 => EBX.clone(),
        4 => ESP.clone(),
        5 => EBP.clone(),
        6 => ESI.clone(),
        7 => EDI.clone(),
        8 => R8D.clone(),
        9 => R9D.clone(),
        10 => R10D.clone(),
        11 => R11D.clone(),
        12 => R12D.clone(),
        13 => R13D.clone(),
        14 => R14D.clone(),
        15 => R15D.clone(),
        _ => unreachable!()
    }
}

pub fn decode_reg64(r_reg: u64) -> Lvalue {
    match r_reg {
        0 => RAX.clone(),
        1 => RCX.clone(),
        2 => RDX.clone(),
        3 => RBX.clone(),
        4 => RSP.clone(),
        5 => RBP.clone(),
        6 => RSI.clone(),
        7 => RDI.clone(),
        8 => R8.clone(),
        9 => R9.clone(),
        10 => R10.clone(),
        11 => R11.clone(),
        12 => R12.clone(),
        13 => R13.clone(),
        14 => R14.clone(),
        15 => R15.clone(),
        _ => unreachable!()
    }
}

pub fn select_reg(os: &OperandSize,r: u64, rex: bool) -> Lvalue {
    match os {
        &OperandSize::Eight => decode_reg8(r,rex),
        &OperandSize::Sixteen => decode_reg16(r),
        &OperandSize::ThirtyTwo => decode_reg32(r),
        &OperandSize::SixtyFour => decode_reg64(r),
        &OperandSize::HundredTwentyEight => panic!("No 128 bit registers in x86!")
    }
}

pub fn select_mem(os: &OperandSize,o: Rvalue,cg: &mut CodeGen<Amd64>) -> Lvalue {
    match os {
        &OperandSize::Eight => {
            rreil!{cg:
                load/ram val:8, (o);
            }
            rreil_lvalue!{ val:8 }
        },
        &OperandSize::Sixteen => {
            rreil!{cg:
                load/ram val:16, (o);
            }
            rreil_lvalue!{ val:16 }
        },
        &OperandSize::ThirtyTwo => {
            rreil!{cg:
                load/ram val:32, (o);
            }
            rreil_lvalue!{ val:32 }
        },
        &OperandSize::SixtyFour => {
            rreil!{cg:
                load/ram val:64, (o);
            }
            rreil_lvalue!{ val:64 }
        },
        &OperandSize::HundredTwentyEight => {
            rreil!{cg:
                load/ram val:128, (o);
            }
            rreil_lvalue!{ val:128 }
        },
    }
}

pub fn decode_modrm(
        _mod: u64,
        b_rm: u64,    // B.R/M
        disp: Option<Rvalue>,
        sib: Option<(u64,u64,u64)>, // scale, X.index, B.base
        os: OperandSize,
        addrsz: AddressSize,
        mode: Mode,
        rex: bool,
        c: &mut CodeGen<Amd64>) -> Option<Lvalue>
{
    match addrsz {
        AddressSize::Sixteen => {
            match _mod {
                0 | 1 | 2 => {
                    let tmp = rreil_lvalue!{ tmp:16 };

                    if b_rm == 6 {
                        if let Some(d) = disp {
                            Some(if _mod == 0 {
                                select_mem(&os,d,c)
                            } else {
                                let t = select_mem(&os,BP.clone().into(),c);
                                rreil!{c:
                                    add (tmp), (t), (d);
                                }
                                tmp
                            })
                        } else {
                            None
                        }
                    } else {
                        let base = select_mem(&os,match b_rm {
                            0 => { rreil!{c: add (tmp), BX:16, SI:16; }; tmp.clone() },
                            1 => { rreil!{c: add (tmp), BX:16, DI:16; }; tmp.clone() },
                            2 => { rreil!{c: add (tmp), BP:16, SI:16; }; tmp.clone() },
                            3 => { rreil!{c: add (tmp), BP:16, DI:16; }; tmp.clone() },
                            4 => SI.clone(),
                            5 => DI.clone(),
                            7 => BX.clone(),
                            _ => unreachable!(),
                        }.into(),c);

                        if _mod == 0 {
                            Some(base)
                        } else {
                            if let Some(ref d) = disp {
                                rreil!{c:
                                    add (tmp), (base), (d);
                                }
                                Some(tmp)
                            } else {
                                None
                            }
                        }
                    }
                },
                3 => Some(select_reg(&os,b_rm,rex)),
                _ => None
            }
        },
        AddressSize::ThirtyTwo | AddressSize::SixtyFour => {
            let maybe_base = match b_rm {
                0 | 1 | 2 | 3 |
                6 | 7 | 8 | 9 | 10 | 11 |
                14 | 15 => Some(select_reg(&if _mod != 3 && addrsz == AddressSize::SixtyFour { OperandSize::SixtyFour } else { os.clone() },b_rm,rex)),

                4 | 12 => if _mod == 3 {
                        Some(select_reg(&os,b_rm,rex))
                    } else {
                        if let Some((scale,index,base)) = sib {
                            decode_sib(_mod,scale,index,base,disp.clone(),os.clone(),c)
                        } else {
                            None
                        }
                    },

                5 | 13 => if _mod == 0 {
                    if let Some(ref d) = disp {
                        Some(if mode == Mode::Long {
                            if addrsz == AddressSize::SixtyFour {
                                let tmp = rreil_lvalue!{ tmp:64 };

                                rreil!{c:
                                    add (tmp), (d), RIP:64;
                                }
                                select_mem(&os,tmp.into(),c)
                            } else {
                                let tmp = rreil_lvalue!{ tmp:32 };

                                rreil!{c:
                                    add (tmp), (d), EIP:32;
                                }
                                select_mem(&os,tmp.into(),c)
                            }
                        } else {
                            select_mem(&os,d.clone(),c)
                        })
                    } else {
                        None
                    }
                } else {
                    Some(select_reg(&if _mod != 3 && addrsz == AddressSize::SixtyFour { OperandSize::SixtyFour } else { os.clone() },b_rm,rex))
                },
                _ => None
            };

            if let Some(base) = maybe_base {
                match _mod {
                    0 => Some(select_mem(&os,base.clone().into(),c)),
                    1 | 2 => {
                        if let Some(ref d) = disp {
                            let tmp = Lvalue::Variable{
                                name: Cow::Borrowed("tmp"),
                                size: os.num_bits(),
                                offset: 0,
                                subscript: None,
                            };

                            rreil!{c:
                                add (tmp), (base), (d);
                            }
                            Some(select_mem(&os,tmp.into(),c))
                        } else {
                            None
                        }
                    },
                    3 => Some(base),
                    _ => None
                }
            } else {
                None
            }
        }
    }
}

fn decode_sib(
    _mod: u64,
    scale: u64,
    x_index: u64,
    b_base: u64,
    disp: Option<Rvalue>,
    os: OperandSize,
    c: &mut CodeGen<Amd64>) -> Option<Lvalue>
{
    match _mod {
        0 => match b_base {
            0 | 1 | 2 | 3 | 4 |
            6 | 7 | 8 | 9 | 10 | 11 | 12 |
            14 | 15 => match x_index {
                0 | 1 | 2 | 3 | 5...15 => {
                    let base = decode_reg64(b_base);
                    let index = decode_reg64(x_index);
                    let s = Rvalue::new_u64((1 << (scale & 3)) / 2);
                    let tmp = rreil_lvalue!{ tmp:64 };

                    Some(if scale > 0 {
                        rreil!{c:
                            mul (tmp), (index), (s);
                            add (tmp), (base), (tmp);
                        }

                        select_mem(&os,tmp.into(),c)
                    } else {
                        rreil!{c:
                            add (tmp), (base), (index);
                        }

                        select_mem(&os,tmp.into(),c)
                    })
                },
                4 => Some(select_mem(&os,Rvalue::new_u64((b_base & 7) as u64),c)),
                _ => None
            },
            5 | 13 => match x_index {
                0...3 | 5...15 => {
                    let index = decode_reg64(x_index);
                    let s = Rvalue::new_u64((1 << (scale & 3)) / 2);
                    let tmp = rreil_lvalue!{ tmp:64 };

                    if let Some(ref d) = disp {
                        Some(if scale > 0 {
                            rreil!{c:
                                mul (tmp), (index), (s);
                                add (tmp), (d), (tmp);
                            }

                            select_mem(&os,tmp.into(),c)
                        } else {
                            rreil!{c:
                                add (tmp), (d), (index);
                            }

                            select_mem(&os,tmp.into(),c)
                        })
                    } else {
                        None
                    }
                },
                4 => if let Some(d) = disp { Some(select_mem(&os,d,c)) } else { None },
                _ => None
            },
            _ => None
        },
        1 | 2 => match x_index {
            0...3 | 5...15 => {
                let base = decode_reg64(b_base);
                let index = decode_reg64(x_index);
                let tmp = rreil_lvalue!{ tmp:64 };
                let s = Rvalue::new_u64((1 << (scale & 3)) / 2);

                if let Some(d) = disp {
                    Some(if scale > 0 {
                        rreil!{c:
                            mul (tmp), (index), (s);
                            add (tmp), (tmp), (d);
                            add (tmp), (base), (tmp);
                        }

                        select_mem(&os,tmp.into(),c)
                    } else {
                        rreil!{c:
                            add (tmp), (index), (d);
                            add (tmp), (base), (tmp);
                        }

                        select_mem(&os,tmp.into(),c)
                    })
                } else {
                    None
                }
            },
            4 => if let Some(d) = disp {
                rreil!{c:
                    add tmp:64, (decode_reg64(b_base)), (d);
                }
                Some(select_mem(&os,rreil_rvalue!{ tmp:64 },c))
            } else {
                None
            },
            _ => None
        },
        _ => None
    }
}

pub fn nonary(opcode: &'static str, sem: fn(&mut CodeGen<Amd64>)) -> Box<Fn(&mut State<Amd64>) -> bool> {
    Box::new(move |st: &mut State<Amd64>| -> bool {
        let len = st.tokens.len();
        let next = st.address + len as u64;

        st.mnemonic_dynargs(len,&opcode,"",&|c| {
            sem(c);
            vec![]
        });
        st.jump(Rvalue::new_u64(next),Guard::always());
        true
    })
}

pub fn unary(opcode: &'static str,
              decode: fn(&mut State<Amd64>) -> Option<Rvalue>,
              sem: fn(&mut CodeGen<Amd64>,Rvalue)
             ) -> Box<Fn(&mut State<Amd64>) -> bool> {
    Box::new(move |st: &mut State<Amd64>| -> bool {
        let len = st.tokens.len();
        let next = st.address + len as u64;
        if let Some(arg) = decode(st) {
            st.mnemonic_dynargs(len,&opcode,"{64}",&|c| {
                sem(c,arg.clone());
                vec![arg.clone()]
            });
            st.jump(Rvalue::new_u64(next),Guard::always());
            true
        } else {
            false
        }
    })
}

pub fn branch(opcode: &'static str,
              decode: fn(&mut State<Amd64>) -> Option<Rvalue>,cc: Condition) -> Box<Fn(&mut State<Amd64>) -> bool> {
    Box::new(move |st: &mut State<Amd64>| -> bool {
        let len = st.tokens.len();
        let next = st.address + len as u64;
        if let Some(arg) = decode(st) {
            st.mnemonic(len,&opcode,"{c}",vec![arg.clone()],&|cg| {
                match cc {
                    Condition::Overflow =>
                        rreil!{cg:
                            mov flag:1, OF:1;
                        },
                    Condition::NotOverflow =>
                        rreil!{cg:
                            xor flag:1, [1]:1, OF:1;
                        },
                    Condition::Carry =>
                        rreil!{cg:
                            mov flag:1, CF:1;
                        },
                    Condition::AboveEqual =>
                        rreil!{cg:
                            xor flag:1, [1]:1, CF:1;
                        },
                    Condition::Equal =>
                        rreil!{cg:
                            mov flag:1, ZF:1;
                        },
                    Condition::NotEqual =>
                        rreil!{cg:
                            xor flag:1, [1]:1, ZF:1;
                        },
                    Condition::BelowEqual =>
                        rreil!{cg:
                            or flag:1, CF:1, ZF:1;
                        },
                    Condition::Above =>
                        rreil!{cg:
                            or flag:1, CF:1, ZF:1;
                            xor flag:1, flag:1, [1]:1;
                        },
                    Condition::Sign =>
                        rreil!{cg:
                            mov flag:1, SF:1;
                        },
                    Condition::NotSign =>
                        rreil!{cg:
                            xor flag:1, [1]:1, SF:1;
                        },
                    Condition::Parity =>
                        rreil!{cg:
                            mov flag:1, PF:1;
                        },
                    Condition::NotParity =>
                        rreil!{cg:
                            xor flag:1, [1]:1, PF:1;
                        },
                    Condition::Less =>
                        rreil!{cg:
                            xor flag:1, SF:1, OF:1;
                            xor flag:1, flag:1, [1]:1;
                        },
                    Condition::GreaterEqual =>
                        rreil!{cg:
                            xor flag:1, SF:1, OF:1;
                        },
                    Condition::LessEqual =>
                        rreil!{cg:
                            xor flag:1, OF:1, SF:1;
                            xor flag:1, flag:1, [1]:1;
                            or flag:1, flag:1, ZF:1;
                        },
                    Condition::Greater =>
                        rreil!{cg:
                            xor flag:1, SF:1, OF:1;
                            xor z:1, [1]:1, ZF:1;
                            or flag:1, flag:1, z:1;
                        },
                }
            });

            let g = Guard::from_flag(&rreil_rvalue!{ flag:1 }).ok().unwrap();
            st.jump(Rvalue::new_u64(next),g.clone());
            st.jump(arg,g.negation());
            true
        } else {
            false
        }
    })
}

pub fn unary_box(opcode: &'static str,
              decode: fn(&mut State<Amd64>) -> Option<Rvalue>,
              sem: Box<Fn(&mut CodeGen<Amd64>,Rvalue)>
             ) -> Box<Fn(&mut State<Amd64>) -> bool> {
    Box::new(move |st: &mut State<Amd64>| -> bool {
        let len = st.tokens.len();
        let next = st.address + len as u64;
        if let Some(arg) = decode(st) {
            st.mnemonic_dynargs(len,&opcode,"{64}",&|c| {
                sem(c,arg.clone());
                vec![arg.clone()]
            });
            st.jump(Rvalue::new_u64(next),Guard::always());
            true
        } else {
            false
        }
    })
}

pub fn unary_c(opcode: &'static str,
                  arg: Rvalue,
                  sem: fn(&mut CodeGen<Amd64>, Rvalue)
                 ) -> Box<Fn(&mut State<Amd64>) -> bool> {
    Box::new(move |st: &mut State<Amd64>| -> bool {
        let len = st.tokens.len();
        let next = st.address + len as u64;

        st.mnemonic_dynargs(len,&opcode,"{64}",&|c| {
            sem(c,arg.clone());
            vec![arg.clone()]
        });
        st.jump(Rvalue::new_u64(next),Guard::always());
        true

    })
}

pub fn binary(opcode: &'static str,
              decode: fn(&mut State<Amd64>) -> Option<(Rvalue,Rvalue)>,
              sem: fn(&mut CodeGen<Amd64>, Rvalue, Rvalue)
             ) -> Box<Fn(&mut State<Amd64>) -> bool> {
    Box::new(move |st: &mut State<Amd64>| -> bool {
        let len = st.tokens.len();
        let next = st.address + len as u64;
        if let Some((arg0,arg1)) = decode(st) {
            st.mnemonic_dynargs(len,&opcode,"{64}, {64}",&|c| {
                sem(c,arg0.clone(),arg1.clone());
                vec![arg0.clone(),arg1.clone()]
            });
            st.jump(Rvalue::new_u64(next),Guard::always());
            true
        } else {
            false
        }
    })
}

pub fn binary_box(opcode: &'static str,
              decode: fn(&mut State<Amd64>) -> Option<(Rvalue,Rvalue)>,
              sem: Box<Fn(&mut CodeGen<Amd64>, Rvalue, Rvalue)>
             ) -> Box<Fn(&mut State<Amd64>) -> bool> {
    Box::new(move |st: &mut State<Amd64>| -> bool {
        let len = st.tokens.len();
        let next = st.address + len as u64;
        if let Some((arg0,arg1)) = decode(st) {
            st.mnemonic_dynargs(len,&opcode,"{64}, {64}",&|c| {
                sem(c,arg0.clone(),arg1.clone());
                vec![arg0.clone(),arg1.clone()]
            });
            st.jump(Rvalue::new_u64(next),Guard::always());
            true
        } else {
            false
        }
    })
}

pub fn binary_rv(opcode: &'static str,
                  _arg0: &Lvalue,
                  decode: fn(&mut State<Amd64>) -> Option<Rvalue>,
                  sem: fn(&mut CodeGen<Amd64>, Rvalue, Rvalue)
                 ) -> Box<Fn(&mut State<Amd64>) -> bool> {
    let arg0 = _arg0.clone();
    Box::new(move |st: &mut State<Amd64>| -> bool {
        let len = st.tokens.len();
        let next = st.address + len as u64;
        if let Some(arg1) = decode(st) {
            st.mnemonic_dynargs(len,&opcode,"{64}, {64}",&|c| {
                sem(c,arg0.clone().into(),arg1.clone());
                vec![arg0.clone().into(),arg1.clone()]
            });
            st.jump(Rvalue::new_u64(next),Guard::always());
            true
        } else {
            false
        }
    })
}

pub fn binary_vr(opcode: &'static str,
                  decode: fn(&mut State<Amd64>) -> Option<Rvalue>,
                  _arg1: &Lvalue,
                  sem: fn(&mut CodeGen<Amd64>, Rvalue, Rvalue)
                 ) -> Box<Fn(&mut State<Amd64>) -> bool> {
    let arg1 = _arg1.clone();
    Box::new(move |st: &mut State<Amd64>| -> bool {
        let len = st.tokens.len();
        let next = st.address + len as u64;
        if let Some(arg0) = decode(st) {
            st.mnemonic_dynargs(len,&opcode,"{64}, {64}",&|c| {
                sem(c,arg0.clone(),arg1.clone().into());
                vec![arg0.clone(),arg1.clone().into()]
            });
            st.jump(Rvalue::new_u64(next),Guard::always());
            true
        } else {
            false
        }
    })
}

pub fn binary_vc(opcode: &'static str,
                  decode: fn(&mut State<Amd64>) -> Option<Rvalue>,
                  arg1: Rvalue,
                  sem: fn(&mut CodeGen<Amd64>, Rvalue, Rvalue)
                 ) -> Box<Fn(&mut State<Amd64>) -> bool> {
    Box::new(move |st: &mut State<Amd64>| -> bool {
        let len = st.tokens.len();
        let next = st.address + len as u64;
        if let Some(arg0) = decode(st) {
            st.mnemonic_dynargs(len,&opcode,"{64}, {64}",&|c| {
                sem(c,arg0.clone(),arg1.clone());
                vec![arg0.clone(),arg1.clone()]
            });
            st.jump(Rvalue::new_u64(next),Guard::always());
            true
        } else {
            false
        }
    })
}

pub fn binary_rr(opcode: &'static str,
                  _arg0: &Lvalue,
                  _arg1: &Lvalue,
                  sem: fn(&mut CodeGen<Amd64>, Rvalue, Rvalue)
                 ) -> Box<Fn(&mut State<Amd64>) -> bool> {
    let arg0 = _arg0.clone();
    let arg1 = _arg1.clone();
    Box::new(move |st: &mut State<Amd64>| -> bool {
        let len = st.tokens.len();
        let next = st.address + len as u64;

        st.mnemonic_dynargs(len,&opcode,"{64}, {64}",&|c| {
            sem(c,arg0.clone().into(),arg1.clone().into());
            vec![arg0.clone().into(),arg1.clone().into()]
        });
        st.jump(Rvalue::new_u64(next),Guard::always());
        true
    })
}

pub fn binary_vv(opcode: &'static str,
                  decodea: fn(&mut State<Amd64>) -> Option<Rvalue>,
                  decodeb: fn(&mut State<Amd64>) -> Option<Rvalue>,
                  sem: fn(&mut CodeGen<Amd64>, Rvalue, Rvalue)
                 ) -> Box<Fn(&mut State<Amd64>) -> bool> {
    Box::new(move |st: &mut State<Amd64>| -> bool {
        let len = st.tokens.len();
        let next = st.address + len as u64;
        if let (Some(arg0),Some(arg1)) = (decodea(st),decodeb(st)) {
            st.mnemonic_dynargs(len,&opcode,"{64}, {64}",&|c| {
                sem(c,arg0.clone(),arg1.clone());
                vec![arg0.clone(),arg1.clone()]
            });
            st.jump(Rvalue::new_u64(next),Guard::always());
            true
        } else {
            false
        }
    })
}

pub fn trinary(opcode: &'static str,
              decode: fn(&mut State<Amd64>) -> Option<(Rvalue,Rvalue,Rvalue)>,
              sem: fn(&mut CodeGen<Amd64>, Rvalue, Rvalue, Rvalue)
             ) -> Box<Fn(&mut State<Amd64>) -> bool> {
    Box::new(move |st: &mut State<Amd64>| -> bool {
        let len = st.tokens.len();
        let next = st.address + len as u64;
        if let Some((arg0,arg1,arg2)) = decode(st) {
            st.mnemonic_dynargs(len,&opcode,"{64}, {64}, {64}",&|c| {
                sem(c,arg0.clone(),arg1.clone(),arg2.clone());
                vec![arg0.clone(),arg1.clone(),arg2.clone()]
            });
            st.jump(Rvalue::new_u64(next),Guard::always());
            true
        } else {
            false
        }
    })
}

pub fn trinary_vr(opcode: &'static str,
                  decode: fn(&mut State<Amd64>) -> Option<(Rvalue,Rvalue)>,
                  c: &Lvalue,
                  sem: fn(&mut CodeGen<Amd64>, Rvalue, Rvalue, Rvalue)) -> Box<Fn(&mut State<Amd64>) -> bool> {
    let arg2 = c.clone();
    Box::new(move |st: &mut State<Amd64>| -> bool {
        let len = st.tokens.len();
        let next = st.address + len as u64;
        if let Some((arg0,arg1)) = decode(st) {
            st.mnemonic_dynargs(len,&opcode,"{64}, {64}, {64}",&|c| {
                sem(c,arg0.clone(),arg1.clone(),arg2.clone().into());
                vec![arg0.clone(),arg1.clone(),arg2.clone().into()]
            });
            st.jump(Rvalue::new_u64(next),Guard::always());
            true
        } else {
            false
        }
    })
}

pub fn quinary(opcode: &'static str,
              decode: fn(&mut State<Amd64>) -> Option<(Rvalue,Rvalue,Rvalue,Rvalue)>,
              sem: fn(&mut CodeGen<Amd64>, Rvalue, Rvalue, Rvalue, Rvalue)
             ) -> Box<Fn(&mut State<Amd64>) -> bool> {
    Box::new(move |st: &mut State<Amd64>| -> bool {
        let len = st.tokens.len();
        let next = st.address + len as u64;
        if let Some((arg0,arg1,arg2,arg3)) = decode(st) {
            st.mnemonic_dynargs(len,&opcode,"{64}, {64}, {64}, {64}",&|c| {
                sem(c,arg0.clone(),arg1.clone(),arg2.clone(),arg3.clone());
                vec![arg0.clone(),arg1.clone(),arg2.clone(),arg3.clone()]
            });
            st.jump(Rvalue::new_u64(next),Guard::always());
            true
        } else {
            false
        }
    })
}

macro_rules! reg {
    ( $a:ident,$I:expr ) => {
        pub fn $a(st: &mut State<Amd64>) -> Option<Rvalue> {
            let r = if st.has_group("b") && st.get_group("b") == 1 { 8 } else { 0 } + $I;
            Some(select_reg(&st.configuration.operand_size,r,st.configuration.rex).clone().into())
        }
    }
}

macro_rules! regd {
    ( $a:ident,$I:expr ) => {
        pub fn $a(st: &mut State<Amd64>) -> Option<Rvalue> {
            let r = if st.has_group("b") && st.get_group("b") == 1 { 8 } else { 0 } + $I;
            let opsz = if st.configuration.mode == Mode::Long && st.configuration.operand_size == OperandSize::ThirtyTwo {
                OperandSize::SixtyFour
            } else {
                st.configuration.operand_size
            };
            Some(select_reg(&opsz,r,st.configuration.rex).clone().into())
        }
    }
}

macro_rules! regb {
    ( $a:ident,$I:expr ) => {
        pub fn $a(st: &mut State<Amd64>) -> Option<Rvalue> {
            let r = if st.has_group("b") && st.get_group("b") == 1 { 8 } else { 0 } + $I;
            Some(select_reg(&OperandSize::Eight,r,st.configuration.rex).clone().into())
        }
    }
}

reg!(reg_a,0);
reg!(reg_c,1);
reg!(reg_d,2);
reg!(reg_b,3);
reg!(reg_sp,4);
reg!(reg_bp,5);
reg!(reg_si,6);
reg!(reg_di,7);

regd!(regd_a,0);
regd!(regd_c,1);
regd!(regd_d,2);
regd!(regd_b,3);
regd!(regd_sp,4);
regd!(regd_bp,5);
regd!(regd_si,6);
regd!(regd_di,7);

regb!(regb_a,0);
regb!(regb_c,1);
regb!(regb_d,2);
regb!(regb_b,3);
regb!(regb_sp,4);
regb!(regb_bp,5);
regb!(regb_si,6);
regb!(regb_di,7);
