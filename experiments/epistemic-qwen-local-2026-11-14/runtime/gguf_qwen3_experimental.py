#!/usr/bin/env python3
"""Experimental Qwen3 GGUF CPU interpreter, proof-of-concept only.
Quantized model weights are extracted/dequantized to float16 mmap files using
NumPy, then layer-by-layer PyTorch FP32 forward passes. NOT a production runtime.
GGUF tensor names and types supported: F32, Q4_K, Q6_K.
"""
import argparse, collections, hashlib, json, math, os, pathlib, re, struct, time
import numpy as np
import torch
import torch.nn.functional as F
import regex

QK = {0:(1,4),12:(256,144),14:(256,210)}
class GGUF:
 def __init__(self,p):
  self.path=pathlib.Path(p); self.fp=self.path.open('rb'); f=self.fp
  assert f.read(4)==b'GGUF'; assert self.u('I')==3
  n=self.u('Q'); k=self.u('Q'); self.meta={}
  for _ in range(k):
   name=self.string(); self.meta[name]=self.value(self.u('I'))
  self.tensors={}
  for _ in range(n):
   name=self.string(); dims=[self.u('Q') for __ in range(self.u('I'))]; ty=self.u('I'); off=self.u('Q'); self.tensors[name]=(dims,ty,off)
  self.base=(f.tell()+31)//32*32
 def u(self,fmt):return struct.unpack('<'+fmt,self.fp.read(struct.calcsize('<'+fmt)))[0]
 def string(self):return self.fp.read(self.u('Q')).decode('utf-8')
 def value(self,t):
  if t==8:return self.string()
  if t==9:
   ty=self.u('I'); return [self.value(ty) for _ in range(self.u('Q'))]
  return self.u({0:'B',1:'b',2:'H',3:'h',4:'I',5:'i',6:'f',7:'?',10:'Q',11:'q',12:'d'}[t])
 def get_raw(self,name):
  dims,ty,off=self.tensors[name]; count=math.prod(dims); sz=count//QK[ty][0]*QK[ty][1]
  return dims,ty,np.memmap(self.path,mode='r',dtype=np.uint8,offset=self.base+off,shape=(sz,))

def dequant_q4k(arr):
 b=arr.reshape(-1,144); d=b[:,0:2].copy().view('<f2').reshape(-1).astype(np.float32); dm=b[:,2:4].copy().view('<f2').reshape(-1).astype(np.float32)
 ss=b[:,4:16]; q=b[:,16:144]; scales=np.empty((len(b),8),dtype=np.float32); mins=np.empty_like(scales)
 for j in range(8):
  if j<4: scales[:,j]=ss[:,j]&63; mins[:,j]=ss[:,j+4]&63
  else:
   scales[:,j]=(ss[:,j+4]&15)|((ss[:,j-4]>>6)<<4)
   mins[:,j]=(ss[:,j+4]>>4)|((ss[:,j]>>6)<<4)
 out=np.empty((len(b),256),dtype=np.float32)
 for i in range(4):
  a=q[:,i*32:(i+1)*32]; j=i*2
  out[:,i*64:i*64+32] = (d*scales[:,j])[:,None]*(a&15).astype(np.float32)-(dm*mins[:,j])[:,None]
  out[:,i*64+32:(i+1)*64] = (d*scales[:,j+1])[:,None]*(a>>4).astype(np.float32)-(dm*mins[:,j+1])[:,None]
 return out.reshape(-1)

def dequant_q6k(arr):
 b=arr.reshape(-1,210); ql=b[:,:128]; qh=b[:,128:192]; scales=b[:,192:208].copy().view('i1').astype(np.float32); d=b[:,208:210].copy().view('<f2').reshape(-1).astype(np.float32)
 out=np.empty((len(b),256),dtype=np.float32)
 for i in range(2):
  a=ql[:,i*64:(i+1)*64]; h=qh[:,i*32:(i+1)*32]; sc=scales[:,i*8:(i+1)*8]
  q1=((a[:,:32]&15)|((h&3)<<4)).astype(np.int16)-32
  q2=((a[:,32:64]&15)|(((h>>2)&3)<<4)).astype(np.int16)-32
  q3=((a[:,:32]>>4)|(((h>>4)&3)<<4)).astype(np.int16)-32
  q4=((a[:,32:64]>>4)|(((h>>6)&3)<<4)).astype(np.int16)-32
  for k,v in enumerate((q1,q2,q3,q4)):
   out[:,i*128+k*32:i*128+(k+1)*32] = d[:,None]*sc[:,k*2+(np.arange(32)//16)]*v.astype(np.float32)
 return out.reshape(-1)

class Store:
 def __init__(self,g,cache):self.g=g; self.cache=pathlib.Path(cache);self.cache.mkdir(parents=True,exist_ok=True)
 def get(self,name):
  dims,ty,_=self.g.tensors[name]; shape=tuple(reversed(dims))
  if ty==0:
   arr=np.memmap(self.g.path,mode='r',dtype='<f4',offset=self.g.base+self.g.tensors[name][2],shape=shape)
   return np.asarray(arr)
  dest=self.cache/(name.replace('/','_')+'.f16')
  if not dest.exists() or dest.stat().st_size != math.prod(dims)*2:
   _,_,raw=self.g.get_raw(name)
   target=np.memmap(str(dest)+'.tmp',mode='w+',dtype='<f2',shape=shape)
   block=QK[ty][1]; block_count=len(raw)//block
   # Do chunks at most 2048 blocks = 524288 float values ~ 1 MiB f16 per chunk
   for st in range(0,block_count,2048):
    upto=min(block_count,st+2048)
    data=np.asarray(raw[st*block:upto*block])
    vals=dequant_q4k(data) if ty==12 else dequant_q6k(data)
    target.reshape(-1)[st*256:upto*256] = vals.astype('<f2')
   target.flush(); del target
   os.replace(str(dest)+'.tmp',dest)
  return np.memmap(dest,mode='r',dtype='<f2',shape=shape)

GPT2_BYTES={}; vals=list(range(ord('!'),ord('~')+1))+list(range(ord('¡'),ord('¬')+1))+list(range(ord('®'),ord('ÿ')+1)); cs=vals[:]; n=0
for b in range(256):
 if b not in vals: vals.append(b);cs.append(256+n);n+=1
GPT2_BYTES=dict(zip(cs,vals)); ENC_BYTES=dict(zip(vals,map(chr,cs)))
PAT=regex.compile(r"(?i:'s|'t|'re|'ve|'m|'ll|'d)|[^\r\n\p{L}\p{N}]?\p{L}+|\p{N}| ?[^\s\p{L}\p{N}]+[\r\n]*|\s*[\r\n]+|\s+(?!\S)|\s+")
class Tokenizer:
 def __init__(self,meta):
  self.tokens=meta['tokenizer.ggml.tokens']; self.ids={v:k for k,v in enumerate(self.tokens)}
  self.merges={tuple(x.split(' ',1)):i for i,x in enumerate(meta['tokenizer.ggml.merges'])}
 def encode(self,txt):
  toks=[]
  for part in PAT.findall(txt):
   pieces=[ENC_BYTES[b] for b in part.encode('utf-8')]
   while len(pieces)>1:
    pairs=[(self.merges.get((pieces[i],pieces[i+1]),10**9),i) for i in range(len(pieces)-1)]
    rank,j=min(pairs)
    if rank==10**9:break
    pieces[j:j+2]=[pieces[j]+pieces[j+1]]
   toks.extend(self.ids[t] for t in pieces)
  return toks
 def decode(self,ids):
  text=''.join(self.tokens[i] for i in ids)
  return bytes(GPT2_BYTES[ord(x)] for x in text).decode('utf-8','replace')

class Runner:
 def __init__(self,g,store):
  self.g=g;self.store=store;self.L=g.meta['qwen3.block_count'];self.h=g.meta['qwen3.attention.head_count'];self.kv=g.meta['qwen3.attention.head_count_kv'];self.d=g.meta['qwen3.embedding_length'];self.dh=self.d//self.h;self.eps=g.meta['qwen3.attention.layer_norm_rms_epsilon'];self.freq=g.meta['qwen3.rope.freq_base'];torch.set_num_threads(4);self.cache=[]; self.embed=None
 def read(self,name):return torch.from_numpy(np.asarray(self.store.get(name)).copy()).float()
 def norm(self,x,w):return F.rms_norm(x,(x.shape[-1],),w,eps=self.eps)
 def mat(self,x,name):return F.linear(x,self.read(name))
 def layer(self,x,i,position):
  pref=f'blk.{i}.'
  a=self.norm(x,self.read(pref+'attn_norm.weight'))
  q=self.mat(a,pref+'attn_q.weight').view(-1,self.h,self.dh).transpose(0,1)
  k=self.mat(a,pref+'attn_k.weight').view(-1,self.kv,self.dh).transpose(0,1)
  v=self.mat(a,pref+'attn_v.weight').view(-1,self.kv,self.dh).transpose(0,1)
  q=self.norm(q,self.read(pref+'attn_q_norm.weight'))
  k=self.norm(k,self.read(pref+'attn_k_norm.weight'))
  invfreq=1/(self.freq**(torch.arange(0,self.dh,2).float()/self.dh)); p=torch.arange(position,position+x.shape[0],dtype=torch.float32); ang=p[:,None]*invfreq[None,:]; cos=torch.cos(ang)[None,:,:];sin=torch.sin(ang)[None,:,:]
  def rope(z):
   lo=z[:,:,:self.dh//2]; hi=z[:,:,self.dh//2:];return torch.cat((lo*cos-hi*sin,hi*cos+lo*sin),dim=-1)
  q=rope(q);k=rope(k)
  if len(self.cache)<=i: self.cache.append((k,v))
  else:
   kp,vp=self.cache[i];k=torch.cat((kp,k),dim=1);v=torch.cat((vp,v),dim=1); self.cache[i]=(k,v)
  # For no cache, prefill should enforce causal. For cached decode, q gets only newest token.
  kfull=k.repeat_interleave(self.h//self.kv,dim=0)
  vfull=v.repeat_interleave(self.h//self.kv,dim=0)
  scores=(q@kfull.transpose(-1,-2))/math.sqrt(self.dh)
  seq=x.shape[0]
  if seq>1:
   query_positions=torch.arange(position,position+seq)[:,None];key_positions=torch.arange(kfull.shape[1])[None,:]
   scores.masked_fill_(key_positions[None,:,:]>query_positions[None,:,:],float('-inf'))
  ctx=F.softmax(scores,dim=-1)@vfull
  attn=ctx.transpose(0,1).reshape(seq,self.d)
  x=x+self.mat(attn,pref+'attn_output.weight')
  a=self.norm(x,self.read(pref+'ffn_norm.weight'))
  x=x+self.mat(F.silu(self.mat(a,pref+'ffn_gate.weight'))*self.mat(a,pref+'ffn_up.weight'),pref+'ffn_down.weight')
  return x
 @torch.no_grad()
 def forward(self,tokens, position, choices=None):
  if self.embed is None: self.embed=self.store.get('token_embd.weight')
  x=torch.from_numpy(np.asarray(self.embed[tokens,:]).copy()).float()
  t0=time.monotonic()
  for i in range(self.L):
   x=self.layer(x,i,position)
   if (i+1)%7==0:print(f'layer {i+1}/{self.L} in {time.monotonic()-t0:.1f}s',flush=True)
  x=self.norm(x[-1:],self.read('output_norm.weight'))
  if choices is not None:
   weights=torch.from_numpy(np.asarray(self.store.get('output.weight')[choices,:]).copy()).float()
   logits=F.linear(x,weights).reshape(-1)
   return int(torch.argmax(logits).item()), [float(y) for y in logits.tolist()]
  logits=self.mat(x,'output.weight').reshape(-1)
  return int(torch.argmax(logits).item()), float(logits.max().item())

def main():
 ap=argparse.ArgumentParser(); ap.add_argument('--model',default='/mnt/data/Qwen3-1.7B-Q4_K_M.gguf');ap.add_argument('--cache',default='/mnt/data/spititout-viper-v0.5/runtime/converted');ap.add_argument('--max-new',type=int,default=3);ap.add_argument('--prompt',default='Responda apenas com o algarismo correto: quanto é 2+2?');a=ap.parse_args();start=time.monotonic()
 g=GGUF(a.model); t=Tokenizer(g.meta)
 txt='<|im_start|>system\nResponda de forma muito curta em português.<|im_end|>\n<|im_start|>user\n'+a.prompt+'<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n'
 # explicit special tokens to avoid splitting multi-character reserved strings
 ids=[]
 for chunk in re.split(r'(<\|(?:im_start|im_end)\|>|<think>|</think>)',txt):
  if chunk: ids.append(t.ids[chunk]) if chunk in t.ids else ids.extend(t.encode(chunk))
 print('prompt_tokens',len(ids),'token_ids_preview',ids[:16],flush=True)
 runner=Runner(g,Store(g,a.cache)); generated=[];pos=0
 for j in range(a.max_new):
  st=time.monotonic(); idx,logit=runner.forward(ids if j==0 else [generated[-1]],pos);pos+=len(ids) if j==0 else 1;generated.append(idx)
  print('generated',j+1,'token',idx,repr(t.decode([idx])),'logit',round(logit,2),'seconds',round(time.monotonic()-st,2),flush=True)
  if idx==g.meta['tokenizer.ggml.eos_token_id']:break
 print('RESULT',json.dumps({'model':str(a.model),'prompt':a.prompt,'generated_tokens':generated,'text':t.decode(generated),'elapsed_s':round(time.monotonic()-start,2)},ensure_ascii=False),flush=True)
if __name__=='__main__':main()
