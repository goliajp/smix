import { Nav } from './components/nav'
import { Capabilities } from './sections/capabilities'
import { Footer } from './sections/footer'
import { Hero } from './sections/hero'
import { Install } from './sections/install'
import { VsMaestro } from './sections/vs-maestro'

export function App() {
  return (
    <div className="min-h-full bg-bg text-fg">
      <Nav />
      <main>
        <Hero />
        <Capabilities />
        <VsMaestro />
        <Install />
      </main>
      <Footer />
    </div>
  )
}
