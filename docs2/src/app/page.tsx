import Link from 'next/link';
import {Button} from '@/components/ui/button';
import {
  TestTube2,
  Wrench,
  ScrollText,
  Search,
  Rocket,
  Github,
  BookOpen,
  Zap
} from 'lucide-react';
import DotGrid from "@/components/Grid";
import {Typewriter} from '@/components/Typewriter';

export default function Home() {
  return (
    <div className="relative min-h-screen flex flex-col bg-black text-white overflow-x-hidden z-0">
      <div style={{
        position: 'absolute',
        top: 0,
        left: -10,
        width: '100%',
        height: '700px',
        zIndex: 0,
        maskImage: 'linear-gradient(to bottom, transparent 0%, black 35%, black 80%, transparent 100%)',
        WebkitMaskImage: 'linear-gradient(to bottom, transparent 0%, black 20%, black 80%, transparent 100%)'
      }}>
        <DotGrid
          dotSize={3}
          gap={30}
          baseColor="#404040"
          activeColor="#5227FF"
          proximity={120}
          shockRadius={250}
          shockStrength={5}
          resistance={750}
          returnDuration={1.5}
        />
      </div>
      <div className="relative z-50 flex-1 flex flex-col">
        <header className="glass-nav sticky top-0 z-50">
          <nav className="container mx-auto px-6 py-4 flex items-center justify-between">
            <div className="text-2xl font-bold tracking-tight flex items-center gap-3">
              {/* <div className="relative w-8 h-8">
                <Image 
                  src="/logo.png" 
                  alt="Acton Logo" 
                  fill
                  className="object-contain"
                />
              </div> */}
              <span className="bg-gradient-to-b from-white via-white to-white/40 bg-clip-text text-transparent">
                Acton
              </span>
            </div>
            <div className="flex gap-8 items-center">
              <Link href="/docs"
                    className="text-sm font-medium text-white/70 hover:text-white transition-colors flex items-center gap-3">
                <BookOpen className="w-4 h-4"/>
                Documentation
              </Link>
              <Link href="https://github.com" target="_blank"
                    className="text-sm font-medium text-white/70 hover:text-white transition-colors flex items-center gap-2">
                <Github className="w-4 h-4"/>
                GitHub
              </Link>
            </div>
          </nav>
        </header>

        <main className="flex-1 flex items-center justify-center px-6 py-50 z-50">
          <div className="container mx-auto max-w-6xl">
            <div className="text-center space-y-16">
              <div className="space-y-8">
                <h1 className="text-6xl md:text-8xl font-bold tracking-tighter">
                  <span className="bg-gradient-to-b from-white via-white to-white/40 bg-clip-text text-transparent">
                    Acton
                  </span>
                </h1>
                <p className="text-3xl md:text-2xl text-white/60 max-w-2.5xl mx-auto font-light leading-relaxed">
                  Blazingly fast <Typewriter words={['toolkit', 'test runner', 'build system', 'formatter', 'verifier']}
                                             className="font-normal" style={{color: "#5227FF"}}/> for TON smart contract
                  development
                </p>
              </div>

              <div className="flex flex-wrap gap-6 justify-center pt-4">
                <Link href="/docs/installation">
                  <Button size="lg"
                          className="glass-button h-12 px-20 rounded-2xl text-base bg-white/10 text-white border border-white/10">
                    Get Started
                  </Button>
                </Link>
                <Link href="https://github.com" target="_blank">
                  <Button size="lg" variant="outline"
                          className="glass-button-outline h-12 px-20 rounded-2xl text-base border-white/10 hover:bg-white/5">
                    <Github className="w-4 h-4 mr-2"/>
                    GitHub
                  </Button>
                </Link>
              </div>

              <div className="grid md:grid-cols-3 gap-8 pt-16 text-left">
                <div className="glass-feature-card rounded-3xl p-8 space-y-4">
                  <div
                    className="w-12 h-12 rounded-2xl bg-white/5 flex items-center justify-center border border-white/10">
                    <Zap className="w-6 h-6 text-amber-300"/>
                  </div>
                  <h3 className="text-xl font-semibold text-white tracking-tight">Lightning Fast</h3>
                  <p className="text-white/50 leading-relaxed">
                    High-performance TON Virtual Machine emulator written in Rust
                  </p>
                </div>

                <div className="glass-feature-card rounded-3xl p-8 space-y-4">
                  <div
                    className="w-12 h-12 rounded-2xl bg-white/5 flex items-center justify-center border border-white/10">
                    <TestTube2 className="w-6 h-6 text-purple-300"/>
                  </div>
                  <h3 className="text-xl font-semibold text-white tracking-tight">Testing</h3>
                  <p className="text-white/50 leading-relaxed">
                    Comprehensive smart contract testing framework with unit and integration tests
                  </p>
                </div>

                <div className="glass-feature-card rounded-3xl p-8 space-y-4">
                  <div
                    className="w-12 h-12 rounded-2xl bg-white/5 flex items-center justify-center border border-white/10">
                    <Wrench className="w-6 h-6 text-blue-300"/>
                  </div>
                  <h3 className="text-xl font-semibold text-white tracking-tight">Build System</h3>
                  <p className="text-white/50 leading-relaxed">
                    Advanced build system with dependency management and code generation
                  </p>
                </div>

                <div className="glass-feature-card rounded-3xl p-8 space-y-4">
                  <div
                    className="w-12 h-12 rounded-2xl bg-white/5 flex items-center justify-center border border-white/10">
                    <ScrollText className="w-6 h-6 text-green-300"/>
                  </div>
                  <h3 className="text-xl font-semibold text-white tracking-tight">Scripting</h3>
                  <p className="text-white/50 leading-relaxed">
                    Powerful scripting capabilities for blockchain interaction and automation
                  </p>
                </div>

                <div className="glass-feature-card rounded-3xl p-8 space-y-4">
                  <div
                    className="w-12 h-12 rounded-2xl bg-white/5 flex items-center justify-center border border-white/10">
                    <Search className="w-6 h-6 text-pink-300"/>
                  </div>
                  <h3 className="text-xl font-semibold text-white tracking-tight">Debugging</h3>
                  <p className="text-white/50 leading-relaxed">
                    Advanced debugging tools with transaction tracing and state inspection
                  </p>
                </div>

                <div className="glass-feature-card rounded-3xl p-8 space-y-4">
                  <div
                    className="w-12 h-12 rounded-2xl bg-white/5 flex items-center justify-center border border-white/10">
                    <Rocket className="w-6 h-6 text-red-300"/>
                  </div>
                  <h3 className="text-xl font-semibold text-white tracking-tight">Deployment</h3>
                  <p className="text-white/50 leading-relaxed">
                    Seamless contract deployment and verification workflows
                  </p>
                </div>
              </div>
            </div>
          </div>
        </main>

        <footer className="glass-nav border-t border-white/10 py-8 mt-auto">
          <div
            className="container mx-auto px-6 flex flex-col md:flex-row justify-between items-center gap-4 text-sm text-white/40">
            <p>© 2025 TON Core. All rights reserved.</p>
          </div>
        </footer>
      </div>
    </div>
  );
}
