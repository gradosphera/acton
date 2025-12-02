import { createMDX } from 'fumadocs-mdx/next';

/** @type {import('next').NextConfig} */
const config = {
    reactStrictMode: true,
    // output: 'export',
    async rewrites() {
        return [
            {
                source: '/docs/:path*.mdx',
                destination: '/llms.mdx/:path*',
            },
        ];
    },
};

const withMDX = createMDX({
    // customise the config file path
    // configPath: "source.config.ts"
});

export default withMDX(config);
