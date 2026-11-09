export const SESSION_TTL=24*60*60*1000;
export const ONLINE_TTL=65_000;
export const SIGNAL_TTL=120_000;
export interface Application {id:string;name:string;type:string;isDeviceFlow:boolean}
export interface ApplicationDirectory {list():Promise<Application[]>}
export interface User {id:string;email?:string}
export interface VerifiedIdentity extends User {clientId:string}
export interface Port {id:string;port:number;type:string;transport:'tcp';tunnelUrl?:string|null}
export interface IrohReachability {endpointId:string;addresses:string[]}
export interface DevicePublication {ports:Port[];tunnelUrl:string|null;certificateFingerprint:string|null;iroh?:IrohReachability|null}
export interface DeviceRow extends DevicePublication {id:string;userId:string;appId:string;name:string;lastSeen:number;iroh:IrohReachability|null;incarnation?:string;publishRevision?:number}
interface SessionBase {hash:string;userId:string;expiresAt:number;protocol?:'memory-v1';incarnation?:string;superseded?:boolean}
export interface DeviceSession extends SessionBase {purpose:'device';appId:string;deviceId:string}
export interface ConsoleSession extends SessionBase {purpose:'console';appId:null;deviceId:null}
export type Session=DeviceSession|ConsoleSession;
export interface Signal {id:number;fromDeviceId:string;connectionId:string;kind:'offer'|'answer'|'candidate'|'close';payload:string;fromIncarnation?:string;toIncarnation?:string;coordinatorEpoch?:string}
export interface StoredSignal extends Signal {toDeviceId:string;expiresAt:number}
export interface Device extends Omit<DeviceRow,'userId'|'appId'|'lastSeen'> {lastSeen:string;online:boolean}
export interface Coordination {protocol:'memory-v1';epoch:string;heartbeatSeconds:20;leaseSeconds:65;signalTtlSeconds:120;maxSignalWaitSeconds:15}
export interface AdminOverviewQuery {page:number;pageSize:number;appId?:string;online?:boolean}
export interface AdminDevice {id:string;appId:string;name:string;online:boolean;lastSeen:string;ports:{port:number;type:string}[];hasTunnel:boolean}
export interface AdminOverview {
 sampledAt:string;
 coordinator:{epoch:string;startedAt:string;uptimeSeconds:number};
 totals:{sessions:number;devices:number;onlineDevices:number;offlineDevices:number;ports:number;queuedSignals:number;waiters:number};
 page:{number:number;size:number;totalDevices:number;totalPages:number;hasNext:boolean};
 devices:AdminDevice[];
}
export interface Store {
 ready?():Promise<void>;
 getRevision?():Promise<number>;
 adminOverview(now:number,query:AdminOverviewQuery):AdminOverview|Promise<AdminOverview>;
 readonly epoch:string;
 readonly revision:number;
 close():void;
 current(s:Session,now:number):void|Promise<void>;
 register(userId:string,appId:string|null,name:string,registrationKey:string|undefined,requestId:string,now:number,registrationRevision?:unknown,expectedEpoch?:string):{session:Session;sessionKey:string}|Promise<{session:Session;sessionKey:string}>;
 publish(s:DeviceSession,update:DevicePublication,incarnation:unknown,revision:unknown,now:number):Promise<void>;
 postSignal(s:DeviceSession,value:Omit<StoredSignal,'id'|'expiresAt'|'fromDeviceId'>,messageId:unknown,now:number):Promise<number>;
 wait(s:DeviceSession,after:number,seconds:number,now:()=>number,signal:AbortSignal):Promise<void>;
 cleanup(now:number):Promise<void>;
 createSession(session:DeviceSession,device:DeviceRow):Promise<void>;
 createConsoleSession(session:ConsoleSession):Promise<void>;
 session(hash:string):Promise<Session|undefined>;
 revoke(hash:string):Promise<void>;
 devices(userId:string,appId:string,now:number,session?:Session):Promise<Device[]>;
 device(id:string,now:number,session?:Session):Promise<(DeviceRow&{online:boolean})|undefined>;
 heartbeat(id:string,update:DevicePublication,now:number):Promise<void>;
 signal(value:Omit<StoredSignal,'id'>):Promise<number>;
 signals(to:string,after:number,now:number,session?:Session):Promise<Signal[]>;
 rate(key:string,now:number,limit:number):Promise<boolean>;
}
export interface AuthConfiguration {provider:'logto';loginFlow?:'authorization_code';issuer:string;clientId:string;resource:string;scope:string;deviceAuthorizationEndpoint:string;tokenEndpoint:string;authorizationEndpoint:string;revocationEndpoint:string;redirectUri:string;jwksUri:string}
export interface Identity {configuration(clientId:string):AuthConfiguration;verify(accessToken:string):Promise<VerifiedIdentity>}
export class ApiError extends Error {constructor(public status:number,public code:string,message:string,public retryAfter?:number){super(message);}}
