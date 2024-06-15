// TextComponent.js
import React from 'react';
import './App.css';
import { CodeBlock , vs2015, atomOneLight, atomOneDark} from "react-code-blocks";


const SnippetCard = ({ title, description, code }: { title: string, description: string; code: string }) => {
  return (
    <div className="w-full h-full">
      <SnippetTitle text={title} />
      <div className="h-1" />
      <SnippetContent text={description} />
      <div className="h-3" />
      <SnippetCode text={code} />
    </div>
  );
};


const SnippetTitle = ({ text }: { text: string }) => {
  return (
    <div className="bg-zinc-900 text-3xl text-white m-0 ">
      {text}
    </div>
  );
  
}

const SnippetContent = ({ text }: { text: string }) => {
  return (
    <div className="bg-zinc-900 text-lg text-white ">
      {text}
    </div>
  );
}
const SnippetCode = ({ text }: { text: string }) => {
  return (
    <div className="text-lg text-white" >
      <CodeBlock
      text={text}
      language='rust'
      showLineNumbers={false}
      theme= {atomOneLight} 
      startingLineNumber={10}
      codeBlock={{ lineNumbers: false, wrapLines: true }}
      
    />
      </div>
  );
};

export default SnippetCard;
