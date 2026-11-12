"""Opt-in Linux/UDisks integration test; writes only a temporary loop device."""
import pathlib, tempfile, subprocess, re
exe = str(pathlib.Path(__file__).resolve().parents[1] / 'target/debug/rustgotek')
def call(args, check=True):
 r = subprocess.run(args, capture_output=True, text=True, timeout=150)
 if check and r.returncode: raise RuntimeError(f'{args}: {r.stdout} {r.stderr}')
 return r
with tempfile.TemporaryDirectory(prefix='rustgotek-loop-check-') as tmp:
 p=pathlib.Path(tmp)
 call([exe,'create-fat12',str(p/'original.img'),'1440'])
 floppy=(p/'original.img').read_bytes(); stride=0x180000
 before=(floppy+b'\xa5'*(stride-len(floppy)))*3
 (p/'device.bin').write_bytes(before)
 (p/'whole-original.bin').write_bytes(before)
 replacement=bytearray(floppy); replacement[43:54]=b'LOOP TEST  '
 (p/'replacement.img').write_bytes(replacement)
 result=call(['udisksctl','loop-setup','--file',str(p/'device.bin'),'--no-user-interaction'])
 match=re.search(r'(/dev/loop[0-9]+)\.?\s*$',result.stdout)
 if not match: raise RuntimeError('Cannot identify the loop device: '+result.stdout)
 loop=match.group(1); print('Temporary device:',loop,flush=True)
 mounted=False
 try:
  call(['udisksctl','mount','--block-device',loop,'--options','ro','--no-user-interaction'])
  mounted=True
  args=[exe,'bank-write',loop,'gotek1440','1',str(p/'replacement.img'),str(p/'original.img'),str(p/'recovery.img'),'--confirm-write']
  refused=call(args,check=False)
  print('Mounted write rejected:',refused.returncode,refused.stderr.strip(),flush=True)
  assert refused.returncode and not (p/'recovery.img').exists()
  assert (p/'device.bin').read_bytes()==before
  restore_args=[exe,'device-restore',loop,str(p/'whole-original.bin'),str(p/'whole-original.bin'),str(p/'whole-recovery.bin'),'--confirm-write']
  assert call(restore_args,check=False).returncode
  assert not (p/'whole-recovery.bin').exists()
  call(['udisksctl','unmount','--block-device',loop,'--no-user-interaction']); mounted=False
  print(call(args).stdout,flush=True)
  after=(p/'device.bin').read_bytes()
  assert after[:stride]==before[:stride]
  assert after[stride:stride+len(floppy)]==replacement
  assert after[stride+len(floppy):]==before[stride+len(floppy):]
  assert (p/'recovery.img').read_bytes()==floppy
  (p/'whole-expected.bin').write_bytes(after)
  restore_args[4]=str(p/'whole-expected.bin')
  print(call(restore_args).stdout,flush=True)
  assert (p/'device.bin').read_bytes()==before
  assert (p/'whole-recovery.bin').read_bytes()==after
  print('Verified bank write, full-device restore, recovery, and mounted-device rejection.',flush=True)
 finally:
  if mounted: call(['udisksctl','unmount','--block-device',loop,'--no-user-interaction'])
  call(['udisksctl','loop-delete','--block-device',loop,'--no-user-interaction'])
