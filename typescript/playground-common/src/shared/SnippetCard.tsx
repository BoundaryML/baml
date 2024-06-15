// TextComponent.js
import React from 'react';
import './App.css';
import { CodeBlock , vs2015} from "react-code-blocks";


const SnippetCard = ({ text: text1, text2 }: { text: string; text2: string }) => {
  return (
    <div className="w-full h-full">
      <SnippetContent text={text1} />
      <div className="h-3" />
      <SnippetCode text={text2} />
    </div>
  );
};


const SnippetContent = ({ text }: { text: string }) => {
  return (
    <div className="bg-zinc-800 text-lg text-white border border-white rounded-md p-4">
      {text}
    </div>
  );
}
const SnippetCode = ({ text }: { text: string }) => {
  return (
    <div className="bg-zinc-800 text-lg text-white border border-white rounded-md p-4" >
      <CodeBlock
      text={text}
      language='rust'
      showLineNumbers={false}
      theme= {vs2015}
      startingLineNumber={10}
      codeBlock={{ lineNumbers: false, wrapLines: true }}
      
    />
      </div>
  );
};

export default SnippetCard;
