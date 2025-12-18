import {Metadata} from 'next';
import Link from 'next/link';
import {ArrowRight, Globe} from 'lucide-react';
import {InstallationCodeBlock} from '@/components/InstallationCodeBlock';

type NavigationLink = {
  label: string;
  shortcut: string;
  href: string;
};

export const metadata: Metadata = {
  title: 'Acton — TON Development Toolkit',
  description:
    'Acton is a blazingly fast toolkit, test runner, build system, formatter, and verifier for TON smart contract development.',
};

const navLinks: NavigationLink[] = [
  {label: 'Docs', shortcut: '[D]', href: '/docs'},
  {label: 'GitHub', shortcut: '[G]', href: 'https://github.com'},
];

const gridBackgroundStyle = {
  backgroundImage: `
    radial-gradient(circle at 20% 25%, rgba(237, 231, 255, 0.6), transparent 32%),
    radial-gradient(circle at 80% 20%, rgba(224, 247, 255, 0.6), transparent 30%),
    radial-gradient(circle at 30% 80%, rgba(255, 241, 223, 0.7), transparent 34%),
    url("data:image/svg+xml,%3Csvg width='288' height='288' viewBox='0 0 288 288' xmlns='http://www.w3.org/2000/svg'%3E%3Cpath d='M144 141v6M141 144h6' stroke='%23999999' stroke-width='1'/%3E%3C/svg%3E")
  `,
  backgroundSize: '100% 100%, 100% 100%, 100% 100%, 288px 288px',
  backgroundPosition: 'center',
};

type Feature = {
  title: string;
  description: string;
  meta: string;
};

const FEATURES: Feature[] = [
  {
    title: 'Native Tolk Testing',
    description:
      'Write integration and unit tests directly in Tolk without TypeScript wrappers. Test individual functions or whole contract systems with a unified API.',
    meta: '[ TESTING ]',
  },
  {
    title: 'Smart Contract Compilation',
    description:
      'Compile Tolk source files to TVM bytecode with incremental caching. Generate source maps for debugging and export compiled code in multiple formats.',
    meta: '[ COMPILER ]',
  },
  {
    title: 'Tolk Formatter',
    description:
      'Automatically format Tolk code with consistent indentation, spacing, and style. Maintain clean, readable code across your entire project.',
    meta: '[ FORMATTER ]',
  },
  {
    title: 'Dependency Management',
    description:
      'Configure contract dependencies in Acton.toml. Choose between code embedding, library references, or storage deployment. Automatic dependency resolution.',
    meta: '[ DEPS ]',
  },
  {
    title: 'Standalone Tolk Scripts',
    description:
      'Execute Tolk files as standalone scripts. Perfect for experimentation, deployment automation, and blockchain interaction using real wallets.',
    meta: '[ SCRIPTS ]',
  },
  {
    title: 'Contract Verification',
    description:
      'Verify deployed contracts match their source code using the TON Verifier service. Supports both testnet and mainnet.',
    meta: '[ VERIFY ]',
  },
  {
    title: 'Blockchain Integration',
    description:
      'Deploy contracts and interact with live blockchain state. Fork mainnet/testnet state for testing, broadcast transactions, and query methods.',
    meta: '[ NETWORK ]',
  },
];

const FigureCard = ({label = 'FIG. 42'}: {label?: string}) => (
  <div className="relative aspect-square w-full max-w-[420px] border border-neutral-300 bg-white/70 shadow-[0_12px_40px_-18px_rgba(0,0,0,0.35)]">
    <div className="absolute inset-2 border border-neutral-200" />
    <div
      className="absolute inset-4 opacity-80"
      style={{
        backgroundImage: `
          repeating-conic-gradient(from 0deg, rgba(0,0,0,0.15) 0deg 2deg, transparent 2deg 8deg),
          radial-gradient(circle at center, transparent 50%, rgba(0,0,0,0.06) 52%),
          radial-gradient(circle at center, rgba(0,0,0,0.08) 1px, transparent 2px)
        `,
        backgroundSize: '100% 100%, 100% 100%, 22px 22px',
      }}
    />
    <div className="absolute inset-0 flex items-center justify-center">
      <span className="rounded border border-black px-2 py-1 text-[10px] font-mono font-semibold bg-white">
        [ {label} ]
      </span>
    </div>
  </div>
);

const FeatureRow = ({feature, index}: {feature: Feature; index: number}) => {
  const isEven = index % 2 === 0;
  return (
    <section
      className={`flex flex-col gap-16 lg:flex-row lg:items-center lg:justify-between ${
        isEven ? 'lg:flex-row-reverse' : ''
      }`}
    >
      <div className="flex-1 space-y-8 lg:max-w-[700px]">
        <div className="space-y-6">
          <MetaLabel text={feature.meta} />
          <h2 className="text-6xl leading-[0.9] tracking-tighter text-neutral-900 sm:text-7xl lg:text-8xl">
            {feature.title}
          </h2>
          <p
            className="text-2xl leading-[1.1] tracking-tight text-neutral-600 sm:text-3xl"
            style={{fontFamily: "'Helvetica Neue', Helvetica, Arial, sans-serif"}}
          >
            {feature.description}
          </p>
        </div>
      </div>
      <div className="flex shrink-0 flex-col items-center justify-center gap-6 lg:w-[500px]">
        <FigureCard label={`FIG. 0${index + 1}`} />
      </div>
    </section>
  );
};

const MetaLabel = ({text}: {text: string}) => (
  <span className="rounded-md border border-neutral-300 bg-white/70 px-2 py-1 text-[10px] font-mono uppercase tracking-widest text-neutral-700 shadow-[0_2px_8px_rgba(0,0,0,0.06)]">
    {text}
  </span>
);

const SectionBadge = ({title}: {title: string}) => (
  <div className="mb-6 flex items-center gap-2">
    <span className="text-sm font-mono text-neutral-500">/</span>
    <span className="text-xs font-mono uppercase tracking-[0.3em] text-neutral-500">
      {title}
    </span>
  </div>
);

export default function Home() {
  return (
    <div
      className="relative min-h-screen overflow-hidden bg-[#f9f9f7] text-neutral-900"
      style={gridBackgroundStyle}
    >
      <div className="absolute inset-0 pointer-events-none bg-[radial-gradient(circle_at_center,_rgba(255,255,255,0.7),_transparent_40%)]" />

      <header className="sticky top-0 z-20 border-b border-neutral-200 bg-white/70 px-4 py-4 backdrop-blur-md">
        <div className="mx-auto flex max-w-[1400px] items-center justify-between">
          <div className="flex flex-wrap items-center gap-2">
            {navLinks.map(link => (
              <Link
                key={link.label}
                href={link.href}
                className="inline-flex items-center gap-2 rounded-full border border-neutral-300 bg-white/70 px-3 py-1 text-[11px] font-mono uppercase tracking-widest text-neutral-700 transition-colors hover:border-neutral-900 hover:text-neutral-900"
              >
                {link.shortcut} {link.label}
              </Link>
            ))}
          </div>
          <Link
            href="/console"
            className="inline-flex items-center gap-2 rounded-full border border-neutral-900 bg-neutral-900 px-4 py-2 text-[11px] font-mono uppercase tracking-widest text-white transition-transform hover:-translate-y-0.5"
          >
            [C] Console
          </Link>
        </div>
      </header>

      <main className="relative z-10">
        <div className="mx-auto flex max-w-[1400px] flex-col gap-48 px-4 pb-48 pt-16">
          <section className="flex flex-col gap-16 lg:flex-row lg:items-center lg:justify-between">
            <div className="flex-1 space-y-12 lg:max-w-[800px]">
              <div className="space-y-8">
                <div className="flex flex-wrap items-center gap-3">
                  <MetaLabel text="[ TEST RUNNER ]" />
                  <MetaLabel text="[ BUILD SYSTEM ]" />
                  <MetaLabel text="[ SCRIPTING ]" />
                </div>
                <h1 className="text-7xl leading-[0.85] tracking-tighter text-neutral-900 sm:text-8xl lg:text-[10rem]">
                  Welcome to
                  <br />
                  <span className="relative inline-flex items-center gap-2">
                    <span className="font-black">Acton</span>
                  </span>
                </h1>
                <p
                  className="text-3xl leading-[1.1] tracking-tight text-neutral-600 sm:text-4xl"
                  style={{fontFamily: "'Helvetica Neue', Helvetica, Arial, sans-serif"}}
                >
                  Build, test, and ship TON smart contracts with a toolkit that keeps
                  you fast and precise—formatter, verifier, test runner, and docs in one
                  place.
                </p>
              </div>
              <div className="w-full max-w-xl">
                <InstallationCodeBlock />
              </div>
            </div>
            <div className="flex shrink-0 flex-col items-center justify-center gap-6 lg:w-[500px]">
              <FigureCard label="FIG. 00" />
            </div>
          </section>

          {FEATURES.map((feature, index) => (
            <FeatureRow key={feature.title} feature={feature} index={index} />
          ))}

          <section className="flex flex-col items-center justify-center gap-12 py-24 text-center">
            <h2 className="text-6xl leading-[0.9] tracking-tighter text-neutral-900 sm:text-7xl lg:text-9xl">
              Ready to build?
            </h2>
            <div className="flex flex-wrap items-center justify-center gap-6">
              <Link
                href="/docs"
                className="inline-flex items-center gap-2 rounded-full border border-neutral-900 bg-neutral-900 px-8 py-4 text-lg font-semibold text-white transition-transform hover:-translate-y-1"
              >
                Get Started
                <ArrowRight className="h-5 w-5" />
              </Link>
              <Link
                href="https://github.com"
                className="inline-flex items-center gap-2 rounded-full border border-neutral-300 bg-white px-8 py-4 text-lg font-semibold text-neutral-900 transition-transform hover:-translate-y-1"
              >
                View GitHub
              </Link>
            </div>
            <div className="w-full max-w-xl">
              <InstallationCodeBlock />
            </div>
          </section>
        </div>
      </main>

      <footer className="relative z-10 mt-10 border-t border-neutral-200 bg-white/70">
        <div className="mx-auto flex max-w-[1400px] flex-col gap-16 px-4 py-16">
          <div className="grid gap-12 lg:grid-cols-[1.2fr,1fr]">
            <div className="space-y-6">
              <SectionBadge title="Docs" />
              <p className="max-w-md text-lg text-neutral-700">
                Explore guides and examples to integrate Acton and deploy TON smart
                contracts with confidence.
              </p>
              <Link
                href="/docs"
                className="inline-flex items-center gap-2 rounded-full border border-neutral-900 px-5 py-3 text-sm font-semibold transition-colors hover:bg-neutral-900 hover:text-white"
              >
                Learn more
                <ArrowRight className="h-4 w-4" />
              </Link>
            </div>
            <div className="grid grid-cols-2 gap-10">
              <div>
                <SectionBadge title="Social" />
                <ul className="space-y-2 text-sm font-mono uppercase tracking-widest text-neutral-700">
                  <li>
                    <Link href="/youtube" className="hover:text-neutral-900">
                      YouTube
                    </Link>
                  </li>
                  <li>
                    <Link href="/twitter" className="hover:text-neutral-900">
                      Twitter/X
                    </Link>
                  </li>
                  <li>
                    <Link href="/discord" className="hover:text-neutral-900">
                      Discord
                    </Link>
                  </li>
                </ul>
              </div>
              <div>
                <SectionBadge title="Resources" />
                <ul className="space-y-2 text-sm font-mono uppercase tracking-widest text-neutral-700">
                  <li>
                    <Link href="/docs" className="hover:text-neutral-900">
                      Docs
                    </Link>
                  </li>
                  <li>
                    <Link href="/meetups" className="hover:text-neutral-900">
                      Developer Meetups
                    </Link>
                  </li>
                </ul>
              </div>
            </div>
          </div>

          <div className="flex flex-col gap-6 border-t border-neutral-100 pt-8 sm:flex-row sm:items-center sm:justify-between">
            <div className="flex items-center gap-4">
              <Globe className="h-8 w-8 text-neutral-900" />
              <span className="text-[11px] font-mono uppercase tracking-[0.3em] text-neutral-600">
                © 2025 Acton, Inc.
              </span>
            </div>
            <div className="flex flex-wrap gap-6 text-[11px] font-mono uppercase tracking-[0.3em] text-neutral-600">
              <Link href="/privacy" className="hover:text-neutral-900">
                Privacy
              </Link>
              <Link href="/legal" className="hover:text-neutral-900">
                Legal
              </Link>
              <Link href="/acton.sh" className="hover:text-neutral-900">
                Acton.sh
              </Link>
            </div>
          </div>

          <div className="pointer-events-none select-none overflow-hidden">
            <h2
              className="text-[15vw] leading-none font-black text-transparent"
              style={{WebkitTextStroke: '1px #dcdcdc'}}
            >
              DEVELOPERS
            </h2>
          </div>
        </div>
      </footer>
    </div>
  );
}
