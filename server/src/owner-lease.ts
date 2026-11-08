import {spawn}from'node:child_process';
import {fileURLToPath}from'node:url';
/** Persistent inode + kernel flock; stdin EOF releases it after parent SIGKILL. */
export async function acquireOwnerLease(path:string){
 const child=spawn('python3',['-u',fileURLToPath(new URL('./owner-lease.py',import.meta.url)),path],{stdio:['pipe','pipe','pipe'],env:{PATH:process.env.PATH??'/usr/bin:/bin',LANG:'C.UTF-8'}});
 let intentional=false,exited=false;let lost=()=>{};const done=new Promise<void>(resolve=>child.once('exit',()=>{exited=true;if(!intentional)lost();resolve();}));
 await new Promise<void>((resolve,reject)=>{let output='',settled=false;const timer=setTimeout(()=>finish(Error('Owner OS lease startup timed out')),5000);const finish=(error?:Error)=>{if(settled)return;settled=true;clearTimeout(timer);child.stdout.removeAllListeners('data');if(error){child.stdin.destroy();child.kill();reject(error);}else resolve();};child.once('error',()=>finish(Error('Python3 POSIX lease helper is required')));child.once('exit',()=>finish(Error('Another owner holds the OS lease or lease path is invalid')));child.stdout.on('data',chunk=>{output+=String(chunk);if(output.length>64)finish(Error('Invalid owner OS lease response'));else if(output==='LOCKED\n')finish();});child.stderr.resume();});
 return {onLost(callback:()=>void){lost=callback;if(exited)callback();},async close(){intentional=true;child.stdin.end();await done;}};
}
