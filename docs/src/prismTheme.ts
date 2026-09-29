import type {PrismTheme} from 'prism-react-renderer';

// Same code colours as the website blog (.blog-prose pre .token.* in blog-refresh.css).
export const nobodywhoDark: PrismTheme = {
  plain: {
    color: '#f4ede6',
    backgroundColor: '#180607',
  },
  styles: [
    {types: ['comment', 'prolog', 'doctype'], style: {color: '#bcaeae'}},
    {types: ['keyword', 'operator', 'atrule', 'selector'], style: {color: '#f3bd79'}},
    {types: ['string', 'attr-value', 'inserted'], style: {color: '#b6d4aa'}},
    {types: ['number', 'boolean', 'constant'], style: {color: '#f3d496'}},
    {types: ['function', 'class-name', 'builtin'], style: {color: '#e2b4d9'}},
    {types: ['variable', 'property', 'parameter', 'attr-name'], style: {color: '#c5d9e3'}},
    {types: ['punctuation'], style: {color: '#d3c7c0'}},
  ],
};

// Light counterpart: the same roles in darker hues, all at least 4.5:1 on the design system's white (#f8f5f0).
export const nobodywhoLight: PrismTheme = {
  plain: {
    color: '#22181a',
    backgroundColor: '#f8f5f0',
  },
  styles: [
    {types: ['comment', 'prolog', 'doctype'], style: {color: '#766a6b'}},
    {types: ['keyword', 'operator', 'atrule', 'selector'], style: {color: '#b8431a'}},
    {types: ['string', 'attr-value', 'inserted'], style: {color: '#466e37'}},
    {types: ['number', 'boolean', 'constant'], style: {color: '#8a5c00'}},
    {types: ['function', 'class-name', 'builtin'], style: {color: '#8a4a80'}},
    {types: ['variable', 'property', 'parameter', 'attr-name'], style: {color: '#3f6475'}},
    {types: ['punctuation'], style: {color: '#5e5254'}},
  ],
};
