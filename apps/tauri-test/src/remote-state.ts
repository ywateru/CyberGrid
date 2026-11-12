export type Layer = { phase: string; reason: string; retryable: boolean };
export type Snapshot = {
  revision: number; generation: number; epoch: string | null;
  identity: Layer; coordination: Layer; publication: Layer; reachability: Layer;
  businessAuthorization: string; remoteEnabled: boolean; intentRevision: number;
  confirmedIntentRevision: number | null; serverPublishRevision: number | null;
};
export type RemoteView = {
  sdkSnapshot: Snapshot | null; remoteAction: string; localClosed: boolean;
  coordinatorRevocation: 'confirmed' | 'pending' | null; directoryKnown: boolean;
};
export function remoteStatus(view: RemoteView) {
  const open = !view.localClosed && view.sdkSnapshot?.remoteEnabled === true;
  const enabling = view.remoteAction === 'enabling';
  const disabling = view.remoteAction === 'disabling';
  const confirmed = open && view.sdkSnapshot?.publication.phase === 'ready'
    && view.sdkSnapshot.confirmedIntentRevision === view.sdkSnapshot.intentRevision;
  const publication = confirmed ? '服务已确认' : !open ? '仅本地意图'
    : view.sdkSnapshot?.coordination.phase !== 'ready' ? '状态未知' : '等待服务确认';
  return { open, enabling, disabling, confirmed, publication,
    showClose: open || enabling || disabling,
    canEnable: !open && !enabling && !disabling,
    canTransfer: open && !disabling && view.directoryKnown
      && view.sdkSnapshot?.coordination.phase === 'ready' };
}
export function layerLabel(layer?: Layer): string {
  return ({ authenticated: '身份已验证', ready: '已就绪', recovering: '恢复中',
    authorizing: '授权中', connecting: '连接中', 'requires-login': '需要授权',
    unavailable: '不可用', disabled: '关闭', pending: '待确认', closed: '关闭',
    disconnected: '未连接', unknown: '未知' } as Record<string, string>)[layer?.phase ?? 'unknown'] ?? '未知';
}
