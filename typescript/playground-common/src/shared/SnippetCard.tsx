// TextComponent.js
import React from 'react';
import './App.css';
import {CodeMirrorViewer} from './CodeMirrorViewer';


const SnippetCard = ({ title, description, code, lang }: { title: string, description: string; code: string; lang:string }) => {
  return (
    <div className="w-full h-full">
      <SnippetTitle text={title} />
      <div className="h-1" />
      <SnippetContent text={description} />
      <div className="h-3" />
      <SnippetCode text={code} lang={lang} />
    </div>
  );
};


const SnippetTitle = ({ text }: { text: string }) => {
  return (
    <div className="bg-zinc-900 text-xl text-white m-0 ">
      {text}
    </div>
  );
  
}

const SnippetContent = ({ text }: { text: string }) => {
  const formattedText = text.replace(/\n/g, '<br/>').replace(/\t/g, '&emsp;');
  return (
    <div className="bg-zinc-900 text-s text-white " dangerouslySetInnerHTML={{ __html: formattedText }} />
  );
}


const SnippetCode = ({ text, lang }: { text: string; lang: string }) => {
  return CodeMirrorViewer({fileContent: text, lang: lang});
};

export default SnippetCard;
