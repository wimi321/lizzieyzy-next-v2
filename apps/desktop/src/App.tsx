import { getMessages } from "./locales";

export default function App() {
  const messages = getMessages(navigator.language);
  return <main><h1>{messages.app.title}</h1></main>;
}
