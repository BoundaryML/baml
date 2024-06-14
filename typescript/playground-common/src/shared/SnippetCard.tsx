// TextComponent.js
import React from 'react';
import './App.css';

const SnippetCard = ({ text: text1, text2 }: { text: string; text2: string }) => {
  return (
    <div className="text-component">
      <div className="text-blob">{text1}</div>
      <div className="text-blob">{text2}</div>
    </div>
  );
};

export default SnippetCard;
