export const browserLoginFailure = '网页登录未完成。请检查授权状态，恢复待处理授权后重试。';

// Native emits a unit enum, not arbitrary exception text or provider responses.
export function browserStartFailureMessage(error: unknown): string {
  if (error === 'EXISTING_AUTHORIZATION') {
    return '此应用已有本机授权，请选择“恢复本机授权”；如需重新网页登录，请先“退出并撤销本机授权”。';
  }
  return browserLoginFailure;
}
