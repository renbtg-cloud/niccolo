import os,sys,unittest
import numpy as np
sys.path.insert(0,os.path.dirname(__file__))
from gguf_qwen3_experimental import GGUF,Tokenizer,dequant_q4k,dequant_q6k

class GGUFTests(unittest.TestCase):
 @classmethod
 def setUpClass(cls):
  cls.g=GGUF(os.environ.get('SPITITOUT_GGUF','/mnt/data/Qwen3-1.7B-Q4_K_M.gguf'));cls.t=Tokenizer(cls.g.meta)
 def test_architecture(self):
  self.assertEqual(self.g.meta['general.architecture'],'qwen3')
  self.assertEqual(self.g.meta['qwen3.block_count'],28)
  self.assertEqual(len(self.g.tensors),311)
 def test_tokenizer_roundtrip(self):
  for s in ['Olá mundo!', '2+2', 'Fala pro meu chefe tomar no cu', 'Não tô conseguindo acompanhar sua explicação.', 'Gíria e ação 🤔']:
   self.assertEqual(self.t.decode(self.t.encode(s)),s)
 def test_q4k_zero(self):
  b=np.zeros(144,dtype='uint8');b[0:2]=np.frombuffer(np.float16(1).tobytes(),dtype=np.uint8)
  b[4:12]=0x11 # quantization scales; mins zero possible depending packing
  self.assertEqual(len(dequant_q4k(b)),256)
  self.assertTrue(np.all(np.isfinite(dequant_q4k(b))))
 def test_q6k_all_zero_codes(self):
  b=np.zeros(210,dtype=np.uint8);b[208:210]=np.frombuffer(np.float16(1).tobytes(),dtype=np.uint8);b[192:208]=1
  v=dequant_q6k(b)
  self.assertEqual(len(v),256)
  np.testing.assert_allclose(v, -32*np.ones(256),rtol=0,atol=0)
 def test_q6k_all_ones_codes(self):
  b=np.zeros(210,dtype=np.uint8);b[208:210]=np.frombuffer(np.float16(1).tobytes(),dtype=np.uint8);b[192:208]=1;b[:192]=255
  v=dequant_q6k(b)
  np.testing.assert_allclose(v, 31*np.ones(256),rtol=0,atol=0)
 def test_gguf_float_norm_shape(self):
  dims,ty,a=self.g.get_raw('output_norm.weight');self.assertEqual(dims,[2048]);self.assertEqual(ty,0);self.assertEqual(len(a),8192)

if __name__=='__main__':unittest.main(verbosity=2)
