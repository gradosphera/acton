export default function Page() {
  const baseUrl = "https://ton-blockchain.github.io/acton/"

  return (
    <>
      <meta httpEquiv="refresh" content={`0; url=${baseUrl}/docs/welcome`} />
      <meta name="robots" content="noindex, follow" />
    </>
  )
}
