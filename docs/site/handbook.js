const search = document.querySelector('#handbook-query');
document.querySelector('#handbook-search').addEventListener('click', () => {
  const target = new URL('https://handbook.dnacalc.com/catalogue/index.html');
  target.searchParams.set('q', search.value.trim());
  location.href = target.href;
});
search.addEventListener('keydown', (event) => {
  if (event.key === 'Enter') document.querySelector('#handbook-search').click();
});
