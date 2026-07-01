'use client';

import { Check, ChevronDown, Play, X } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';

export function IterateTestDemo() {
  const [isRunning, setIsRunning] = useState(false);
  const [showResponse, setShowResponse] = useState(false);

  const handleRunTest = () => {
    setIsRunning(true);
    setTimeout(() => {
      setIsRunning(false);
      setShowResponse(true);
    }, 1800);
  };

  return (
    <div className="w-full max-w-6xl mx-auto rounded-lg border bg-white shadow-lg overflow-hidden">
      {/* Playground Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b bg-gray-50">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium text-gray-700">
            BAML Playground
          </span>
          <button className="p-1 hover:bg-gray-200 rounded" type="button">
            <X className="h-4 w-4 text-gray-500" />
          </button>
        </div>
      </div>

      {/* Function and Input Selectors */}
      <div className="flex items-center gap-4 px-4 py-3 border-b bg-gray-50">
        <div className="flex items-center gap-2">
          <span className="text-sm text-gray-600">📋</span>
          <button
            className="flex items-center gap-2 px-3 py-1.5 text-sm font-medium text-gray-700 hover:bg-gray-100 rounded"
            type="button"
          >
            ExtractResume
            <ChevronDown className="h-3 w-3" />
          </button>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-sm text-gray-600">🧪</span>
          <button
            className="flex items-center gap-2 px-3 py-1.5 text-sm font-medium text-gray-700 hover:bg-gray-100 rounded"
            type="button"
          >
            vaibhav_resume
            <ChevronDown className="h-3 w-3" />
          </button>
        </div>
        <div className="ml-auto">
          <Button
            className="bg-purple-600 hover:bg-purple-700 text-white px-6"
            disabled={isRunning}
            onClick={handleRunTest}
          >
            <Play className="h-4 w-4 mr-2" />
            {isRunning ? 'Running...' : 'Run Test'}
          </Button>
        </div>
      </div>

      {/* Code Editor */}
      <div className="bg-gray-900 text-gray-100 font-mono text-sm">
        {/* Tab Bar */}
        <div className="flex items-center border-b border-gray-800 px-4">
          <div className="px-4 py-2 border-b-2 border-purple-500 text-white">
            Preview
          </div>
          <div className="px-4 py-2 text-gray-500">cURL</div>
          <div className="px-4 py-2 text-gray-500">Client Graph</div>
        </div>

        {/* Editor Content */}
        <div className="p-4 space-y-4">
          <div>
            <div className="text-gray-500 mb-2">System</div>
            <div className="pl-4">
              <div className="text-gray-300">Extract from this content:</div>
              <div className="mt-2 bg-blue-900/20 border border-blue-800/30 rounded p-2 text-blue-300">
                Vaibhav Gupta
              </div>
              <div className="mt-1 bg-blue-900/20 border border-blue-800/30 rounded p-2 text-blue-300">
                vbv@boundaryml.com
              </div>
            </div>
          </div>

          <div className="pl-4 space-y-2">
            <div className="bg-blue-900/20 border border-blue-800/30 rounded p-2">
              <div className="text-blue-300 mb-1">Experience:</div>
              <div className="text-gray-300 pl-2">- Founder at BoundaryML</div>
              <div className="text-gray-300 pl-2">- CV Engineer at Google</div>
              <div className="text-gray-300 pl-2">
                - CV Engineer at Microsoft
              </div>
            </div>

            <div className="bg-blue-900/20 border border-blue-800/30 rounded p-2">
              <div className="text-blue-300 mb-1">Skills:</div>
              <div className="text-gray-300 pl-2">- Rust</div>
              <div className="text-gray-300 pl-2">- C++</div>
            </div>
          </div>
        </div>
      </div>

      {/* Response Section */}
      {showResponse && (
        <div className="border-t bg-gray-50">
          {/* Response Header */}
          <div className="flex items-center gap-6 px-4 py-3 border-b bg-white text-sm">
            <div className="flex items-center gap-2">
              <Play className="h-4 w-4 text-purple-600" />
            </div>
            <div className="flex items-center gap-2 text-gray-600">
              <span>📋</span>
              <span>ExtractResume</span>
            </div>
            <div className="flex items-center gap-2 text-gray-600">
              <span>🧪</span>
              <span>vaibhav_resume</span>
            </div>
            <div className="flex items-center gap-2 text-green-600 ml-auto">
              <Check className="h-4 w-4" />
              <span>Passed</span>
            </div>
          </div>

          {/* Metadata */}
          <div className="flex items-center gap-6 px-4 py-2 text-sm text-gray-600 bg-white border-b">
            <div className="flex items-center gap-1">
              <span>🤖</span>
              <span>gpt-4o-2024-08-06</span>
            </div>
            <div className="flex items-center gap-1">
              <span>⏱️</span>
              <span>1.86s</span>
            </div>
            <div className="flex items-center gap-1">
              <span>🔤</span>
              <span>81 in | 72 out</span>
            </div>
          </div>

          {/* Parsed Response */}
          <div className="p-4 bg-gray-900 text-gray-100 font-mono text-sm">
            <h3 className="text-gray-400 mb-3">Parsed Response</h3>
            <div className="pl-4">
              <div className="text-gray-300">
                <span className="text-gray-500">{'{ '}</span>
                <span className="text-gray-500 italic">4 items</span>
              </div>
              <div className="pl-4">
                <div>
                  <span className="text-purple-400">"name"</span>
                  <span className="text-gray-500">: </span>
                  <span className="text-green-400">"Vaibhav Gupta"</span>
                  <span className="text-gray-500">,</span>
                </div>
                <div>
                  <span className="text-purple-400">"email"</span>
                  <span className="text-gray-500">: </span>
                  <span className="text-green-400">"vbv@boundaryml.com"</span>
                  <span className="text-gray-500">,</span>
                </div>
                <div>
                  <span className="text-purple-400">"experience"</span>
                  <span className="text-gray-500">: [</span>
                  <span className="text-gray-500 italic"> 3 items</span>
                  <div className="pl-4">
                    <div className="text-green-400">
                      "Founder at BoundaryML",
                    </div>
                    <div className="text-green-400">
                      "CV Engineer at Google",
                    </div>
                    <div className="text-green-400">
                      "CV Engineer at Microsoft",
                    </div>
                  </div>
                  <span className="text-gray-500">],</span>
                </div>
                <div>
                  <span className="text-purple-400">"skills"</span>
                  <span className="text-gray-500">: [</span>
                  <span className="text-gray-500 italic"> 2 items</span>
                  <div className="pl-4">
                    <div className="text-green-400">"Rust",</div>
                    <div className="text-green-400">"C++",</div>
                  </div>
                  <span className="text-gray-500">]</span>
                </div>
              </div>
              <div className="text-gray-500">{'}'}</div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
