/** @type {import('next').NextConfig} */
const nextConfig = {
  // node:crypto is a Node built-in (auto-external in the node runtime). The data
  // layer now talks to Neon Postgres over HTTP (@neondatabase/serverless), so no
  // special config is needed.
};
export default nextConfig;
