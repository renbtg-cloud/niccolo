import unittest
from challenge_001 import CASES, PARSER_CASES, K, B, R, NOT, P, Q, deduct, explain, item_prompt
class ContractTests(unittest.TestCase):
 def test_cases(self):
  seen=set()
  for x in CASES:
   self.assertNotIn(x[0],seen);seen.add(x[0]);out,_=deduct(x[1],x[2]);self.assertIn(out,('SUPPORTED','REFUTED','UNRESOLVED'))
  self.assertEqual(len(CASES),15)
 def test_factivity(self):
  self.assertEqual(deduct([K('X',P,'t0')],P)[0],'SUPPORTED')
  self.assertEqual(deduct([K('X',K('Y',P,'t1'),'t2')],P)[0],'SUPPORTED')
 def test_nonfactivity(self):
  self.assertEqual(deduct([B('X',P,'t0')],P)[0],'UNRESOLVED')
  self.assertEqual(deduct([R('X','Y',P,'t0')],P)[0],'UNRESOLVED')
 def test_refutation(self):
  self.assertEqual(deduct([NOT(P)],P)[0],'REFUTED')
  self.assertEqual(deduct([P],NOT(P))[0],'REFUTED')
  self.assertEqual(deduct([K('X',NOT(Q),'t0')],Q)[0],'REFUTED')
 def test_no_time_persistence(self):
  self.assertEqual(deduct([K('X',P,'t0')],K('X',P,'t1'))[0],'UNRESOLVED')
 def test_labels_and_inputs(self):
  for i,c in enumerate(CASES):
   p1,m1=item_prompt(c,'direct',i)
   p2,m2=item_prompt(c,'challenge',i)
   self.assertEqual(m1,m2)
   self.assertEqual(set(m1.values()),{'SUPPORTED','REFUTED','UNRESOLVED'})
   for fact in c[1]:
    self.assertIn(explain(fact),p1)
    self.assertIn(explain(fact),p2)
 def test_parser_balance(self):
  self.assertEqual(len(PARSER_CASES),12)
  self.assertEqual({t:sum(x[2]==t for x in PARSER_CASES) for t in ('KNOWLEDGE','BELIEF','COMMUNICATION')},dict.fromkeys(('KNOWLEDGE','BELIEF','COMMUNICATION'),4))
if __name__=='__main__': unittest.main()
