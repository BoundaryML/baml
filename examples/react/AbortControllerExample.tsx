import React, { useState, useEffect, useRef } from 'react';
import { b } from 'baml_client';
import { AbortError } from '@boundaryml/baml';

interface StreamResult {
  content: string;
  isPartial: boolean;
}

export function StreamingComponent() {
  const [results, setResults] = useState<StreamResult[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const streamRef = useRef<any>(null);
  const abortControllerRef = useRef<AbortController | null>(null);
  
  // Function to start the streaming process
  const startStream = async (useExternalController = false) => {
    setIsStreaming(true);
    setError(null);
    setResults([]);
    
    try {
      let stream;
      
      if (useExternalController) {
        // Method 2: Using external AbortController
        abortControllerRef.current = new AbortController();
        stream = b.stream.YourFunction(
          { param1: 'value1' }, 
          { signal: abortControllerRef.current.signal }
        );
      } else {
        // Method 1: Using stream's built-in abort method
        stream = b.stream.YourFunction({ param1: 'value1' });
      }
      
      streamRef.current = stream;
      
      // Process stream results
      for await (const partial of stream) {
        setResults(prev => [...prev, { content: JSON.stringify(partial), isPartial: true }]);
      }
      
      // Get final result
      const final = await stream.getFinalResponse();
      setResults(prev => [...prev, { content: JSON.stringify(final), isPartial: false }]);
    } catch (error) {
      if (error instanceof AbortError) {
        setError('Stream was aborted');
      } else {
        setError(`Stream error: ${error.message}`);
        console.error('Stream error:', error);
      }
    } finally {
      setIsStreaming(false);
      streamRef.current = null;
    }
  };
  
  // Function to abort the stream using Method 1
  const handleDirectAbort = () => {
    if (streamRef.current) {
      streamRef.current.abort();
      streamRef.current = null;
    }
  };
  
  // Function to abort the stream using Method 2
  const handleControllerAbort = () => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
  };
  
  // Clean up on unmount
  useEffect(() => {
    return () => {
      if (streamRef.current) {
        streamRef.current.abort();
      }
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  }, []);
  
  return (
    <div className="streaming-container">
      <h2>BAML Stream with AbortController</h2>
      
      <div className="controls">
        <button 
          onClick={() => startStream(false)} 
          disabled={isStreaming}
          className="start-button"
        >
          Start Stream (Direct Abort)
        </button>
        
        <button 
          onClick={() => startStream(true)} 
          disabled={isStreaming}
          className="start-button alt"
        >
          Start Stream (Controller Abort)
        </button>
        
        <button 
          onClick={handleDirectAbort} 
          disabled={!isStreaming}
          className="abort-button"
        >
          Abort Stream Directly
        </button>
        
        <button 
          onClick={handleControllerAbort} 
          disabled={!isStreaming || !abortControllerRef.current}
          className="abort-button alt"
        >
          Abort via Controller
        </button>
      </div>
      
      {isStreaming && (
        <div className="status streaming">
          Streaming in progress...
          {streamRef.current && (
            <span className="abort-status">
              Is aborted: {streamRef.current.isAborted ? 'Yes' : 'No'}
            </span>
          )}
        </div>
      )}
      
      {error && (
        <div className="status error">
          {error}
        </div>
      )}
      
      <div className="results-container">
        <h3>Results:</h3>
        {results.length === 0 ? (
          <div className="no-results">No results yet</div>
        ) : (
          <div className="results-list">
            {results.map((result, i) => (
              <div 
                key={i} 
                className={`result-item ${result.isPartial ? 'partial' : 'final'}`}
              >
                <span className="result-badge">
                  {result.isPartial ? 'Partial' : 'Final'}
                </span>
                <pre>{result.content}</pre>
              </div>
            ))}
          </div>
        )}
      </div>
      
      <style jsx>{`
        .streaming-container {
          font-family: system-ui, -apple-system, sans-serif;
          max-width: 800px;
          margin: 0 auto;
          padding: 20px;
        }
        
        .controls {
          display: flex;
          flex-wrap: wrap;
          gap: 10px;
          margin-bottom: 20px;
        }
        
        button {
          padding: 8px 16px;
          border-radius: 4px;
          font-weight: 500;
          cursor: pointer;
        }
        
        button:disabled {
          opacity: 0.6;
          cursor: not-allowed;
        }
        
        .start-button {
          background-color: #4CAF50;
          color: white;
          border: none;
        }
        
        .start-button.alt {
          background-color: #2196F3;
        }
        
        .abort-button {
          background-color: #f44336;
          color: white;
          border: none;
        }
        
        .abort-button.alt {
          background-color: #FF9800;
        }
        
        .status {
          padding: 10px;
          border-radius: 4px;
          margin-bottom: 20px;
          display: flex;
          justify-content: space-between;
          align-items: center;
        }
        
        .streaming {
          background-color: #e3f2fd;
          border: 1px solid #bbdefb;
        }
        
        .error {
          background-color: #ffebee;
          border: 1px solid #ffcdd2;
          color: #c62828;
        }
        
        .abort-status {
          font-size: 14px;
          font-style: italic;
        }
        
        .results-container {
          border: 1px solid #e0e0e0;
          border-radius: 4px;
          padding: 15px;
        }
        
        .no-results {
          color: #757575;
          font-style: italic;
        }
        
        .results-list {
          display: flex;
          flex-direction: column;
          gap: 10px;
        }
        
        .result-item {
          padding: 10px;
          border-radius: 4px;
          position: relative;
        }
        
        .partial {
          background-color: #f5f5f5;
          border-left: 3px solid #9e9e9e;
        }
        
        .final {
          background-color: #e8f5e9;
          border-left: 3px solid #4caf50;
        }
        
        .result-badge {
          position: absolute;
          top: 0;
          right: 0;
          font-size: 12px;
          padding: 2px 6px;
          border-radius: 0 4px 0 4px;
          color: white;
        }
        
        .partial .result-badge {
          background-color: #9e9e9e;
        }
        
        .final .result-badge {
          background-color: #4caf50;
        }
        
        pre {
          margin: 0;
          white-space: pre-wrap;
          word-break: break-word;
        }
      `}</style>
    </div>
  );
}
