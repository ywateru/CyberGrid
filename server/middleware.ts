import {next, rewrite} from '@vercel/functions';

// Vercel's root middleware convention applies this response policy before static
// routing as well as API routing. Never replace a page with a policy response.
export const config = {runtime: 'nodejs'};

export function issuerOrigin(value: string | undefined): string | undefined {
  if (!value || value.trim() !== value) return;
  try {
    const url = new URL(value);
    if (url.protocol !== 'https:' || url.username || url.password || url.search || url.hash) return;
    return url.origin;
  } catch { return; }
}

export default function middleware(request: Request): Response {
  const origin = issuerOrigin(process.env.LOGTO_ISSUER);
  const headers = new Headers({
    'Content-Security-Policy': "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'" + (origin ? ' ' + origin : '') + "; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
  });
  const requested = new URL(request.url);
  if (requested.pathname === '/admin/' || requested.pathname.startsWith('/admin/')) headers.set('Cache-Control', 'no-store');
  const configuredCallback = process.env.CYBERGRID_CONSOLE_REDIRECT_URI;
  const publicOrigin = issuerOrigin(process.env.CYBERGRID_PUBLIC_ORIGIN);
  if (configuredCallback && publicOrigin) {
    try {
      const callback = new URL(configuredCallback);
      if (callback.protocol === 'https:' && callback.origin === publicOrigin && !callback.username && !callback.password && !callback.search && !callback.hash && callback.pathname !== '/' && !/^\/(?:api|admin)(?:\/|$)/.test(callback.pathname) && callback.pathname === requested.pathname) {
        const destination = new URL('/console/callback/', requested);
        destination.search = requested.search;
        return rewrite(destination, {headers});
      }
    } catch { /* Invalid custom configuration does not block ordinary pages. */ }
  }
  return next({headers});
}
