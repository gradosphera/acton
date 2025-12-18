'use client';

import React, {useState} from 'react';
import {Copy, Check} from 'lucide-react';

const TABS = ['macOS / Linux', 'Windows'];
const COMMANDS: Record<string, string> = {
  'macOS / Linux': '<span class="font-bold">curl</span> -fsSL https://acton.sh/install | <span class="font-bold">bash</span>',
  'Windows': '<span class="font-bold">powershell</span> -c "irm acton.sh/install.ps1 | iex"',
};

export const InstallationCodeBlock: React.FC = () => {
  const [activeTab, setActiveTab] = useState(TABS[0]);
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    void navigator.clipboard.writeText(COMMANDS[activeTab].replace(/<[^>]*>/g, ''));
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleTabChange = (tab: string) => {
    setActiveTab(tab);
    setCopied(false);
  };

  return (
    <div className="relative max-w-xl">
      <div className="relative bg-white border border-gray-200 rounded-lg shadow-sm overflow-hidden">
        <div className="flex border-b border-gray-100 bg-gray-50/50 px-2 pt-2">
          {TABS.map(tab => (
            <button
              key={tab}
              onClick={() => handleTabChange(tab)}
              className={`px-4 py-2 text-xs font-mono uppercase tracking-widest transition-colors focus:outline-none rounded-t-lg ${
                activeTab === tab
                  ? 'text-black bg-white border-x border-t border-gray-200 -mb-[1px]'
                  : 'text-gray-400 hover:text-gray-600'
              }`}
            >
              {tab}
            </button>
          ))}
        </div>
        <div className="p-6 flex items-center justify-between bg-white">
          <div className="flex items-center gap-4 overflow-x-auto">
            <span className="text-gray-300 select-none text-sm font-mono">$</span>
            <code
              className="text-gray-800 font-mono text-sm whitespace-nowrap"
              dangerouslySetInnerHTML={{__html: COMMANDS[activeTab]}}
            />
          </div>
          <button onClick={handleCopy}
                  className="text-gray-400 hover:text-black transition-colors p-2 rounded-md shrink-0 hover:cursor-pointer">
            {copied ? <Check className="w-4 h-4 text-green-500"/> : <Copy className="w-4 h-4"/>}
          </button>
        </div>
      </div>
    </div>
  );
};
