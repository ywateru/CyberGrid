import type {TrustedBridgeAdapter} from '../../../../sdk-js/src/index.js';
export interface WebConfig {
 readonly server:string; readonly issuer:string; readonly clientId:string;
 readonly resource:string; readonly productId:string; readonly redirectUri:string;
}
/** Private capability passed only to the deliberately trusted adapter constructor. */
export interface BusinessAuthorization { fresh(options:{signal:AbortSignal}):Promise<string> }
export type SampleRequest =
 | {kind:'info'} | {kind:'list-sessions'} | {kind:'poll'}
 | {kind:'create-session'; value:string};
export type SampleResponse = Readonly<{revision?:number;values?:readonly string[];name?:string;version?:number;count?:number}>;
export type SampleStreamInput = Readonly<{kind:'echo';sessionId:string}>;
export interface WebDependencies {
 readonly createAdapter?:(authorization:BusinessAuthorization)=>TrustedBridgeAdapter<SampleRequest,SampleResponse,SampleStreamInput>;
}
