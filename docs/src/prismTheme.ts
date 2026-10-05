import type {PrismTheme} from 'prism-react-renderer';

// Syntax colours from the website blog (.blog-prose pre .token.* in blog-refresh.css),
// on the design system's ink-dark-3 to sit on the ink-black page.
export const nobodywhoDark: PrismTheme = {
  plain: {
    color: '#f4ede6',
    backgroundColor: '#1e1618',
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

// Light counterpart: the same roles in darker hues, all at least 7:1 on the design system's white (#f8f5f0).
// Comments are italic because at that contrast they sit close to the punctuation grey.
export const nobodywhoLight: PrismTheme = {
  plain: {
    color: '#22181a',
    backgroundColor: '#f8f5f0',
  },
  styles: [
    {types: ['comment', 'prolog', 'doctype'], style: {color: '#5a5151', fontStyle: 'italic'}},
    {types: ['keyword', 'operator', 'atrule', 'selector'], style: {color: '#923515'}},
    {types: ['string', 'attr-value', 'inserted'], style: {color: '#3a5b2e'}},
    {types: ['number', 'boolean', 'constant'], style: {color: '#704b00'}},
    {types: ['function', 'class-name', 'builtin'], style: {color: '#763f6e'}},
    {types: ['variable', 'property', 'parameter', 'attr-name'], style: {color: '#375766'}},
    {types: ['punctuation'], style: {color: '#5b5052'}},
  ],
};
