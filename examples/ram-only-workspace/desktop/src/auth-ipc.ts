import {Channel,invoke} from '@tauri-apps/api/core';
import {RamOwner,OwnerFailure,type Binding,type TokenRequest,type Failure,type NativeResponse} from './ram-owner.ts';
import {jwtChecks} from './native-jwt.ts';
type Reply={requestId:string;ownerGeneration:number;sdkGeneration:number;accessToken:string|null;failure:Failure|null};
export class DesktopAuth {
 #owner:RamOwner|undefined;#delivery=new Map<string,Promise<void>>();#replies=new Map<string,Reply>();#channel:Channel<TokenRequest>|undefined;#operation=0;
 private async drain():Promise<void>{await Promise.all([...this.#delivery.values()].map(p=>p.catch(()=>{})));for(const [id,reply]of this.#replies){await invoke('token_reply',{reply});this.#delivery.delete(id);this.#replies.delete(id);}}
 async login():Promise<void>{
  if(this.#owner)throw Error('已有本机授权，请先退出或重新连接。');const operation=++this.#operation;
  const begin=await invoke<{ownerGeneration:number;binding:Binding}>('owner_begin');const generation=begin.ownerGeneration;
  if(operation!==this.#operation){await invoke('owner_end',{ownerGeneration:generation});throw Error('登录已取消。');}
  const owner=new RamOwner(generation,begin.binding,{fetch:(operationId,request)=>invoke<NativeResponse>('oauth_fetch',{ownerGeneration:generation,operationId,request}),navigate:url=>invoke('oauth_open',{ownerGeneration:generation,url}),...jwtChecks(begin.binding)});this.#owner=owner;
  const channel=new Channel<TokenRequest>();this.#channel=channel;channel.onmessage=request=>{
   if(this.#delivery.has(request.requestId))return;
   if(this.#delivery.size>=2){void invoke('token_reply',{reply:{requestId:request.requestId,ownerGeneration:generation,sdkGeneration:request.sdkGeneration,accessToken:null,failure:'BUSY'}}).catch(()=>{});return;}
   const job=(async()=>{let accessToken:string|null=null,failure:Failure|null=null;try{accessToken=await owner.accessToken(request);}catch(e){failure=e instanceof OwnerFailure?e.code:'AUTH_UNAVAILABLE';}
    const reply:Reply={requestId:request.requestId,ownerGeneration:generation,sdkGeneration:request.sdkGeneration,accessToken,failure};this.#replies.set(request.requestId,reply);
    // Failed acknowledgements retain the exact reply; drain retries only IPC, never OAuth.
    await invoke('token_reply',{reply});this.#replies.delete(request.requestId);
   })();this.#delivery.set(request.requestId,job);void job.then(()=>this.#delivery.delete(request.requestId),()=>{});
  };
  await invoke('owner_attach',{ownerGeneration:generation,channel});await owner.signIn();const callback=await invoke<string>('oauth_callback',{ownerGeneration:generation});if(operation!==this.#operation)throw Error('登录已取消。');await owner.callback(callback);await this.connect();
 }
 async connect():Promise<void>{const owner=this.#owner;if(!owner)throw Error('请先登录。');await this.drain();await owner.settled();const sdkGeneration=await invoke<number>('sdk_prepare',{ownerGeneration:owner.generation});await owner.bindSdk(sdkGeneration);await invoke('sdk_login',{ownerGeneration:owner.generation,sdkGeneration});}
 async logout():Promise<void>{++this.#operation;await invoke('remote_disable');const owner=this.#owner;if(!owner)return;await this.drain();await owner.settled();await invoke('owner_end',{ownerGeneration:owner.generation});await owner.end();this.#owner=undefined;this.#channel=undefined;}
}
