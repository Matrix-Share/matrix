import { NextRequest, NextResponse } from 'next/server';

/**
 * Lightweight edge guard: bounce signed-out visitors away from app pages before
 * they render (the real auth check + user load still happens in the (app) layout,
 * which can reach the database; the edge runtime cannot). Only checks for the
 * presence of the session cookie.
 */
const PROTECTED = ['/dashboard', '/team', '/billing', '/settings', '/admin'];

export function middleware(req: NextRequest) {
  const { pathname } = req.nextUrl;
  if (PROTECTED.some((p) => pathname === p || pathname.startsWith(p + '/'))) {
    if (!req.cookies.get('lifeline_session')) {
      const url = req.nextUrl.clone();
      url.pathname = '/login';
      return NextResponse.redirect(url);
    }
  }
  return NextResponse.next();
}

export const config = {
  matcher: ['/dashboard/:path*', '/team/:path*', '/billing/:path*', '/settings/:path*', '/admin/:path*'],
};
