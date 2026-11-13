import '@fontsource-variable/inter-tight';
import './styles/app.css';
import { createRoot } from 'react-dom/client';
import App from './App';

const root = document.getElementById('root');

if (!root) {
  throw new Error('The app root element is missing.');
}

createRoot(root).render(<App />);
