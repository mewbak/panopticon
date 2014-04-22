#include <iostream>
#include <algorithm>
#include <iterator>
#include <stdexcept>

#include <boost/graph/isomorphism.hpp>
#include <gtest/gtest.h>
#include <panopticon/procedure.hh>
#include <panopticon/disassembler.hh>
#include "architecture.hh"

using namespace po;
using namespace boost;

class disassembler_mockup : public po::disassembler<test_tag>
{
public:
	disassembler_mockup(const std::map<typename po::architecture_traits<test_tag>::token_type,po::sem_state<test_tag>> &states)
	: m_states(states) {}

	virtual boost::optional<typename po::rule<test_tag>::tokiter> match(typename po::rule<test_tag>::tokiter begin, typename po::rule<test_tag>::tokiter end, po::sem_state<test_tag> &state) const
	{
		if(begin == end)
			return boost::none;

		auto i = m_states.find(*begin);

		if(i != m_states.end())
		{
			state.mnemonics = i->second.mnemonics;
			state.jumps = i->second.jumps;

			return std::next(begin,std::accumulate(state.mnemonics.begin(),state.mnemonics.end(),0,[](size_t acc, const po::mnemonic &m) { return icl::size(m.area) + acc; }));
		}
		else
			return boost::none;
	}

private:
	std::map<typename po::architecture_traits<test_tag>::token_type,po::sem_state<test_tag>> m_states;
};

TEST(procedure,add_single)
{
	std::vector<typename po::architecture_traits<test_tag>::token_type> bytes({0});
	std::map<typename po::architecture_traits<test_tag>::token_type,po::sem_state<test_tag>> states;

	{
		po::sem_state<test_tag> st(0);
		st.mnemonic(1,"test");
		states.insert(std::make_pair(0,st));
	}

	disassembler_mockup mockup(states);
	po::proc_loc proc = po::procedure::disassemble(0,mockup,bytes,0);

	ASSERT_EQ(proc->rev_postorder().size(), 1);

	po::bblock_loc bb = *proc->rev_postorder().begin();

	ASSERT_EQ(bb->mnemonics().size(), 1);
	ASSERT_EQ(bb->mnemonics()[0].opcode, "test");
	ASSERT_EQ(bb->mnemonics()[0].area, po::bound(0,1));
	ASSERT_EQ(po::bound(0,1), bb->area());
	ASSERT_EQ(bb, *(proc->entry));
	ASSERT_EQ(num_edges(proc->control_transfers), 0);
	ASSERT_EQ(num_vertices(proc->control_transfers), 1);
	ASSERT_NE(proc->name, "");
}

TEST(procedure,continuous)
{
	std::vector<typename po::architecture_traits<test_tag>::token_type> bytes({0,1,2,3,4,5});
	std::map<typename po::architecture_traits<test_tag>::token_type,po::sem_state<test_tag>> states;
	auto add = [&](po::offset p, const std::string &n) -> void
	{
		po::sem_state<test_tag> st(p);
		st.mnemonic(1,n);
		st.jump(p+1);
		states.insert(std::make_pair(p,st));
	};
	auto check = [&](const po::mnemonic &m, const std::string &n, po::offset p) -> void
	{
		ASSERT_EQ(m.opcode, n);
		ASSERT_EQ(m.operands.size(), 0);
		ASSERT_EQ(m.instructions.size(), 0);
		ASSERT_EQ(m.area, po::bound(p,p+1));
	};

	add(0,"test0");
	add(1,"test1");
	add(2,"test2");
	add(3,"test3");
	add(4,"test4");
	add(5,"test5");

	disassembler_mockup mockup(states);
	po::proc_loc proc = po::procedure::disassemble(0,mockup,bytes,0);

	ASSERT_TRUE(!!proc->entry);
	ASSERT_EQ(proc->rev_postorder().size(), 1);

	po::bblock_loc bb = *proc->rev_postorder().begin();

	ASSERT_EQ(bb->mnemonics().size(), 6);

	check(bb->mnemonics()[0],"test0",0);
	check(bb->mnemonics()[1],"test1",1);
	check(bb->mnemonics()[2],"test2",2);
	check(bb->mnemonics()[3],"test3",3);
	check(bb->mnemonics()[4],"test4",4);
	check(bb->mnemonics()[5],"test5",5);

	auto ep = edges(proc->control_transfers);
	using edge_descriptor = boost::graph_traits<decltype(procedure::control_transfers)>::edge_descriptor;
	ASSERT_TRUE(std::all_of(ep.first,ep.second,[&](edge_descriptor e) { try { get_edge(e,proc->control_transfers); return true; } catch(...) { return false; } }));

	auto in_p = incoming(proc,bb);
	auto out_p = outgoing(proc,bb);

	ASSERT_EQ(distance(in_p.first,in_p.second), 0);
	ASSERT_EQ(distance(out_p.first,out_p.second), 1);
	ASSERT_TRUE(get_edge(*out_p.first,proc->control_transfers).relations.empty());
	ASSERT_TRUE(is_constant(get<rvalue>(get_node(target(*out_p.first,proc->control_transfers),proc->control_transfers))));
	ASSERT_EQ(to_constant(get<rvalue>(get_node(target(*out_p.first,proc->control_transfers),proc->control_transfers))).content(), 6);
	ASSERT_EQ(bb->area(), po::bound(0,6));
	ASSERT_EQ(bb, *(proc->entry));
	ASSERT_NE(proc->name, "");
}

TEST(procedure,branch)
{
	std::vector<typename po::architecture_traits<test_tag>::token_type> bytes({0,1,2});
	std::map<typename po::architecture_traits<test_tag>::token_type,po::sem_state<test_tag>> states;
	auto add = [&](po::offset p, const std::string &n, po::offset b1, boost::optional<po::offset> b2) -> void
	{
		po::sem_state<test_tag> st(p);
		st.mnemonic(1,n);
		st.jump(b1);
		if(b2)
			st.jump(*b2);
		states.insert(std::make_pair(p,st));
	};
	auto check = [&](const po::mnemonic &m, const std::string &n, po::offset p) -> void
	{
		ASSERT_EQ(m.opcode, n);
		ASSERT_TRUE(m.operands.empty());
		ASSERT_TRUE(m.instructions.empty());
		ASSERT_EQ(m.area, po::bound(p,p+1));
	};

	add(0,"test0",1,2);
	add(1,"test1",3,boost::none);
	add(2,"test2",1,boost::none);

	disassembler_mockup mockup(states);
	po::proc_loc proc = po::procedure::disassemble(0,mockup,bytes,0);

	ASSERT_EQ(proc->rev_postorder().size(), 3);

	auto i0 = std::find_if(proc->rev_postorder().begin(),proc->rev_postorder().end(),[&](po::bblock_loc bb) { return bb->area().lower() == 0; });
	auto i1 = std::find_if(proc->rev_postorder().begin(),proc->rev_postorder().end(),[&](po::bblock_loc bb) { return bb->area().lower() == 1; });
	auto i2 = std::find_if(proc->rev_postorder().begin(),proc->rev_postorder().end(),[&](po::bblock_loc bb) { return bb->area().lower() == 2; });

	ASSERT_NE(i0, proc->rev_postorder().end());
	ASSERT_NE(i1, proc->rev_postorder().end());
	ASSERT_NE(i2, proc->rev_postorder().end());

	po::bblock_loc bb0 = *i0;
	po::bblock_loc bb1 = *i1;
	po::bblock_loc bb2 = *i2;

	ASSERT_EQ(bb0->mnemonics().size(), 1);
	ASSERT_EQ(bb1->mnemonics().size(), 1);
	ASSERT_EQ(bb2->mnemonics().size(), 1);

	auto in0_p = incoming(proc,bb0);
	auto out0_p = outgoing(proc,bb0);

	ASSERT_EQ(distance(in0_p.first,in0_p.second), 0);
	check(bb0->mnemonics()[0],"test0",0);
	ASSERT_EQ(distance(out0_p.first,out0_p.second), 2);

	auto in1_p = incoming(proc,bb1);
	auto out1_p = outgoing(proc,bb1);

	ASSERT_EQ(distance(in1_p.first,in1_p.second), 2);
	check(bb1->mnemonics()[0],"test1",1);
	ASSERT_EQ(distance(out1_p.first,out1_p.second), 1);

	auto in2_p = incoming(proc,bb2);
	auto out2_p = outgoing(proc,bb2);

	ASSERT_EQ(distance(in2_p.first,in2_p.second), 1);
	check(bb2->mnemonics()[0],"test2",2);
	ASSERT_EQ(distance(out2_p.first,out2_p.second), 1);
}

TEST(procedure,loop)
{
	std::vector<typename po::architecture_traits<test_tag>::token_type> bytes({0,1,2});
	std::map<typename po::architecture_traits<test_tag>::token_type,po::sem_state<test_tag>> states;
	auto add = [&](po::offset p, const std::string &n, po::offset b1) -> void
	{
		po::sem_state<test_tag> st(p);
		st.mnemonic(1,n);
		st.jump(b1);
		states.insert(std::make_pair(p,st));
	};
	auto check = [&](const po::mnemonic &m, const std::string &n, po::offset p) -> void
	{
		ASSERT_EQ(m.opcode, n);
		ASSERT_TRUE(m.operands.empty());
		ASSERT_TRUE(m.instructions.empty());
		ASSERT_EQ(m.area, po::bound(p,p+1));
	};

	add(0,"test0",1);
	add(1,"test1",2);
	add(2,"test2",0);

	disassembler_mockup mockup(states);
	po::proc_loc proc = po::procedure::disassemble(0,mockup,bytes,0);

	ASSERT_EQ(proc->rev_postorder().size(), 1);

	po::bblock_loc bb = *proc->rev_postorder().begin();

	ASSERT_EQ(bb->mnemonics().size(), 3);

	check(bb->mnemonics()[0],"test0",0);
	check(bb->mnemonics()[1],"test1",1);
	check(bb->mnemonics()[2],"test2",2);

	auto in_p = incoming(proc,bb);
	auto out_p = outgoing(proc,bb);

	ASSERT_EQ(distance(in_p.first,in_p.second), 1);
	ASSERT_EQ(distance(out_p.first,out_p.second), 1);
}

TEST(procedure,empty)
{
	std::vector<typename po::architecture_traits<test_tag>::token_type> bytes({});
	std::map<typename po::architecture_traits<test_tag>::token_type,po::sem_state<test_tag>> states;
	disassembler_mockup mockup(states);
	po::proc_loc proc = po::procedure::disassemble(0,mockup,bytes,0);

	ASSERT_EQ(proc->rev_postorder().size(), 0);
}

TEST(procedure,refine)
{
	std::vector<typename po::architecture_traits<test_tag>::token_type> bytes({0,1,2});
	std::map<typename po::architecture_traits<test_tag>::token_type,po::sem_state<test_tag>> states;
	auto add = [&](po::offset p, size_t l, const std::string &n, po::offset b1) -> void
	{
		po::sem_state<test_tag> st(p);
		st.mnemonic(l,n);
		st.jump(b1);
		states.insert(std::make_pair(p,st));
	};
	auto check = [&](const po::mnemonic &m, const std::string &n, po::offset p) -> void
	{
		ASSERT_EQ(m.opcode, n);
		ASSERT_TRUE(m.operands.empty());
		ASSERT_TRUE(m.instructions.empty());
		ASSERT_EQ(m.area, po::bound(p,p+1));
	};

	/*
	 * test0
	 *  -"-  test1
	 * test2
	 */
	add(0,2,"test0",2);
	add(2,1,"test2",1);
	add(1,1,"test1",2);

	disassembler_mockup mockup(states);
	po::proc_loc proc = po::procedure::disassemble(0,mockup,bytes,0);

	ASSERT_EQ(proc->rev_postorder().size(), 2);

	auto i0 = std::find_if(proc->rev_postorder().begin(),proc->rev_postorder().end(),[&](po::bblock_loc bb) { return bb->area().lower() == 0; });
	auto i1 = std::find_if(proc->rev_postorder().begin(),proc->rev_postorder().end(),[&](po::bblock_loc bb) { return bb->area().lower() == 1; });

	ASSERT_NE(i0, proc->rev_postorder().end());
	ASSERT_NE(i1, proc->rev_postorder().end());

	po::bblock_loc bb0 = *i0;
	po::bblock_loc bb1 = *i1;

	ASSERT_EQ(bb0->mnemonics().size(), 1);
	ASSERT_EQ(bb1->mnemonics().size(), 2);

	check(bb0->mnemonics()[0],"test0",0);
	check(bb1->mnemonics()[0],"test1",1);
	check(bb1->mnemonics()[1],"test2",2);

	auto in0_p = incoming(proc,bb0);
	auto out0_p = outgoing(proc,bb0);

	ASSERT_EQ(distance(in0_p.first,in0_p.second), 0);
	ASSERT_EQ(distance(out0_p.first,out0_p.second), 1);

	auto in1_p = incoming(proc,bb1);
	auto out1_p = outgoing(proc,bb1);

	ASSERT_EQ(distance(in1_p.first,in1_p.second), 2);
	ASSERT_EQ(distance(out1_p.first,out1_p.second), 1);

}

TEST(procedure,continue)
{
	po::proc_loc proc(new po::procedure(""));
	po::mnemonic mne0(po::bound(0,1),"test0","",{},{});
	po::mnemonic mne1(po::bound(1,2),"test1","",{},{});
	po::mnemonic mne2(po::bound(2,3),"test2","",{},{});
	po::mnemonic mne3(po::bound(6,7),"test6","",{},{});
	po::bblock_loc bb0(new po::basic_block());
	po::bblock_loc bb1(new po::basic_block());
	po::bblock_loc bb2(new po::basic_block());

	bb0.write().mnemonics().push_back(mne0);
	bb0.write().mnemonics().push_back(mne1);
	bb1.write().mnemonics().push_back(mne2);
	bb2.write().mnemonics().push_back(mne3);

	unconditional_jump(proc,bb0,po::constant(42));
	unconditional_jump(proc,bb2,po::constant(40));

	po::unconditional_jump(proc,bb0,bb1);
	po::unconditional_jump(proc,bb0,bb2);

	proc.write().entry = bb0;

	std::vector<typename po::architecture_traits<test_tag>::token_type> bytes({0,1,2,0,0,0,6,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,40,41,42});
	std::map<typename po::architecture_traits<test_tag>::token_type,po::sem_state<test_tag>> states;
	auto add = [&](po::offset p, const std::string &n, boost::optional<po::offset> b1, boost::optional<po::offset> b2) -> void
	{
		po::sem_state<test_tag> st(p);
		st.mnemonic(1,n);
		if(b1)
			st.jump(*b1);
		if(b2)
			st.jump(*b2);

		states.insert(std::make_pair(p,st));
	};
	auto check = [&](const po::mnemonic &m, const std::string &n, po::offset p) -> void
	{
		ASSERT_EQ(m.opcode, n);
		ASSERT_TRUE(m.operands.empty());
		ASSERT_TRUE(m.instructions.empty());
		ASSERT_EQ(m.area, po::bound(p,p+1));
	};

	add(0,"test0",1,boost::none);
	add(1,"test1",2,6);
	add(2,"test2",boost::none,boost::none);
	add(6,"test6",40,boost::none);

	add(40,"test40",41,boost::none);
	add(41,"test41",42,boost::none);
	add(42,"test42",55,make_optional<offset>(0));

	disassembler_mockup mockup(states);
	proc = po::procedure::disassemble(proc,mockup,bytes,40);

	ASSERT_EQ(proc->rev_postorder().size(), 4);

	auto i0 = std::find_if(proc->rev_postorder().begin(),proc->rev_postorder().end(),[&](po::bblock_loc bb) { return bb->area().lower() == 0; });
	auto i1 = std::find_if(proc->rev_postorder().begin(),proc->rev_postorder().end(),[&](po::bblock_loc bb) { return bb->area().lower() == 2; });
	auto i2 = std::find_if(proc->rev_postorder().begin(),proc->rev_postorder().end(),[&](po::bblock_loc bb) { return bb->area().lower() == 6; });
	auto i3 = std::find_if(proc->rev_postorder().begin(),proc->rev_postorder().end(),[&](po::bblock_loc bb) { return bb->area().lower() == 40; });

	ASSERT_NE(i0, proc->rev_postorder().end());
	ASSERT_NE(i1, proc->rev_postorder().end());
	ASSERT_NE(i2, proc->rev_postorder().end());
	ASSERT_NE(i3, proc->rev_postorder().end());

	po::bblock_loc bbo0 = *i0;
	po::bblock_loc bbo1 = *i1;
	po::bblock_loc bbo2 = *i2;
	po::bblock_loc bbo3 = *i3;
	auto ct = proc->control_transfers;

	auto in0_p = incoming(proc,bbo0);
	auto out0_p = outgoing(proc,bbo0);

	ASSERT_EQ(distance(in0_p.first,in0_p.second), 1);
	ASSERT_TRUE(get<bblock_loc>(get_node(target(*in0_p.first,ct),ct)) == bbo3);
	ASSERT_EQ(bbo0->mnemonics().size(), 2);
	check(bbo0->mnemonics()[0],"test0",0);
	check(bbo0->mnemonics()[1],"test1",1);
	ASSERT_EQ(distance(out0_p.first,out0_p.second), 2);
	ASSERT_TRUE(get<bblock_loc>(get_node(target(*out0_p.first,ct),ct)) == bbo1 || get<bblock_loc>(get_node(target(*out0_p.first,ct),ct)) == bbo2);
	ASSERT_TRUE(get<bblock_loc>(get_node(target(*(out0_p.first+1),ct),ct)) == bbo1 || get<bblock_loc>(get_node(target(*(out0_p.first+1),ct),ct)) == bbo2);

	auto in1_p = incoming(proc,bbo1);
	auto out1_p = outgoing(proc,bbo1);

	ASSERT_EQ(distance(in1_p.first,in1_p.second), 1);
	ASSERT_TRUE(get<bblock_loc>(get_node(target(*in1_p.first,ct),ct)) == bbo0);
	ASSERT_EQ(bbo1->mnemonics().size(), 1);
	check(bbo1->mnemonics()[0],"test2",2);
	ASSERT_EQ(distance(out1_p.first,out1_p.second), 0);

	auto in2_p = incoming(proc,bbo2);
	auto out2_p = outgoing(proc,bbo2);

	ASSERT_EQ(distance(in2_p.first,in2_p.second), 1);
	ASSERT_TRUE(get<bblock_loc>(get_node(target(*in2_p.first,ct),ct)) == bbo0);
	ASSERT_EQ(bbo2->mnemonics().size(), 1);
	check(bbo2->mnemonics()[0],"test6",6);
	ASSERT_EQ(distance(out2_p.first,out2_p.second), 1);
	ASSERT_TRUE(get<bblock_loc>(get_node(target(*out2_p.first,ct),ct)) == bbo3);

	auto in3_p = incoming(proc,bbo3);
	auto out3_p = outgoing(proc,bbo3);

	ASSERT_EQ(distance(in3_p.first,in3_p.second), 1);
	ASSERT_TRUE(get<bblock_loc>(get_node(target(*in3_p.first,ct),ct)) == bbo2);
	ASSERT_EQ(bbo3->mnemonics().size(), 3);
	check(bbo3->mnemonics()[0],"test40",40);
	check(bbo3->mnemonics()[1],"test41",41);
	check(bbo3->mnemonics()[2],"test42",42);
	ASSERT_EQ(distance(out3_p.first,out3_p.second), 2);
	ASSERT_TRUE(get<bblock_loc>(get_node(target(*out3_p.first,ct),ct)) == bbo0 || to_constant(get<rvalue>(get_node(target(*out3_p.first,ct),ct))).content() == 55);
	ASSERT_TRUE(get<bblock_loc>(get_node(target(*(out3_p.first+1),ct),ct)) == bbo0 || to_constant(get<rvalue>(get_node(target(*(out3_p.first+1),ct),ct))).content() == 55);

	ASSERT_EQ(*(proc->entry), bbo0);
}

TEST(procedure,entry_split)
{
	po::proc_loc proc(new po::procedure(""));
	po::mnemonic mne0(po::bound(0,1),"test0","",{},{});
	po::mnemonic mne1(po::bound(1,2),"test1","",{},{});
	po::bblock_loc bb0(new po::basic_block());

	bb0.write().mnemonics().push_back(mne0);
	bb0.write().mnemonics().push_back(mne1);
	unconditional_jump(proc,bb0,po::constant(2));

	proc.write().entry = bb0;

	std::vector<typename po::architecture_traits<test_tag>::token_type> bytes({0,1,2});
	std::map<typename po::architecture_traits<test_tag>::token_type,po::sem_state<test_tag>> states;
	auto add = [&](po::offset p, const std::string &n, boost::optional<po::offset> b1, boost::optional<po::offset> b2) -> void
	{
		po::sem_state<test_tag> st(p);
		st.mnemonic(1,n);
		if(b1)
			st.jump(*b1);
		if(b2)
			st.jump(*b2);

		states.insert(std::make_pair(p,st));
	};
	auto check = [&](const po::mnemonic &m, const std::string &n, po::offset p) -> void
	{
		ASSERT_EQ(m.opcode, n);
		ASSERT_TRUE(m.operands.empty());
		ASSERT_TRUE(m.instructions.empty());
		ASSERT_EQ(m.area, po::bound(p,p+1));
	};

	add(0,"test0",1,boost::none);
	add(1,"test1",2,boost::none);

	add(2,"test2",1,boost::none);

	disassembler_mockup mockup(states);
	proc = po::procedure::disassemble(proc,mockup,bytes,2);

	ASSERT_EQ(proc->rev_postorder().size(), 2);

	auto i0 = std::find_if(proc->rev_postorder().begin(),proc->rev_postorder().end(),[&](po::bblock_loc bb) { return bb->area().lower() == 0; });
	auto i1 = std::find_if(proc->rev_postorder().begin(),proc->rev_postorder().end(),[&](po::bblock_loc bb) { return bb->area().lower() == 1; });

	ASSERT_NE(i0, proc->rev_postorder().end());
	ASSERT_NE(i1, proc->rev_postorder().end());

	po::bblock_loc bbo0 = *i0;
	po::bblock_loc bbo1 = *i1;

	ASSERT_EQ(*(proc->entry), bbo0);
	ASSERT_EQ(bbo0->mnemonics().size(), 1);
	check(bbo0->mnemonics()[0],"test0",0);
	ASSERT_EQ(bbo1->mnemonics().size(), 2);
}

TEST(procedure,variable)
{
	ASSERT_TRUE(false);
}

/*
 *   bb0 ----+
 *    |  \   |
 *   bb1  a  |
 *   /  \    |
 *   bb2 \   |
 *   \   /   |
 * +-bb3---2 |
 * +/ |      |
 *    bb4----+
 */
TEST(procedure,marshal)
{
	bblock_loc bb0(new basic_block({mnemonic(bound(0,5),"test","",{},{})}));
	bblock_loc bb1(new basic_block({mnemonic(bound(5,10),"test","",{},{})}));
	bblock_loc bb2(new basic_block({mnemonic(bound(10,12),"test","",{},{})}));
	bblock_loc bb3(new basic_block({mnemonic(bound(12,20),"test","",{},{})}));
	bblock_loc bb4(new basic_block({mnemonic(bound(20,21),"test","",{},{})}));
	rvalue rv1 = variable("a",8);
	rvalue rv2 = constant(42);
	proc_loc proc(new procedure("p1"));

	auto vx0 = insert_node<variant<bblock_loc,rvalue>,guard>(bb0,proc.write().control_transfers);
	auto vx1 = insert_node<variant<bblock_loc,rvalue>,guard>(bb1,proc.write().control_transfers);
	auto vx2 = insert_node<variant<bblock_loc,rvalue>,guard>(bb2,proc.write().control_transfers);
	auto vx3 = insert_node<variant<bblock_loc,rvalue>,guard>(bb3,proc.write().control_transfers);
	auto vx4 = insert_node<variant<bblock_loc,rvalue>,guard>(bb4,proc.write().control_transfers);
	auto vx5 = insert_node<variant<bblock_loc,rvalue>,guard>(rv1,proc.write().control_transfers);
	auto vx6 = insert_node<variant<bblock_loc,rvalue>,guard>(rv2,proc.write().control_transfers);

	insert_edge(guard(),vx0,vx1,proc.write().control_transfers);
	insert_edge(guard(),vx0,vx5,proc.write().control_transfers);
	insert_edge(guard(),vx1,vx2,proc.write().control_transfers);
	insert_edge(guard(),vx2,vx3,proc.write().control_transfers);
	insert_edge(guard(),vx1,vx3,proc.write().control_transfers);
	insert_edge(guard(),vx3,vx3,proc.write().control_transfers);
	insert_edge(guard(),vx3,vx6,proc.write().control_transfers);
	insert_edge(guard(),vx3,vx4,proc.write().control_transfers);
	insert_edge(guard(),vx4,vx0,proc.write().control_transfers);

	proc.write().entry = bb0;

	rdf::storage st;
	save_point(st);

	std::unique_ptr<procedure> p2(unmarshal<procedure>(proc.tag(),st));

	ASSERT_EQ(proc->name, p2->name);
	ASSERT_TRUE(**proc->entry == **p2->entry);
	ASSERT_TRUE(boost::isomorphism(proc->control_transfers,p2->control_transfers));
	ASSERT_EQ(proc->rev_postorder(), p2->rev_postorder());
}
