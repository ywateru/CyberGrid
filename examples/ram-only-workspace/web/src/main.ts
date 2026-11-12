import {mountRemoteWorkspace,deliverCallback,type WebConfig} from './index.ts';
import './style.css';
// Private licensed integrations import mountRemoteWorkspace and supply a trusted factory.
// The public executable deliberately has no business adapter.
if(!deliverCallback()){
 const env=import.meta.env;
 const config:WebConfig={server:env.VITE_CYBERGRID_SERVER??'https://cybergrid.aixgg.com',issuer:env.VITE_LOGTO_ISSUER??'https://kn3wnj.logto.app/oidc',clientId:env.VITE_LOGTO_SPA_CLIENT??'',resource:env.VITE_LOGTO_RESOURCE??'https://cybergrid.aixgg.com/api',productId:env.VITE_CYBERGRID_PRODUCT??'',redirectUri:location.origin+'/auth/callback'};
 mountRemoteWorkspace(document.querySelector<HTMLElement>('#app')!,config);
}
