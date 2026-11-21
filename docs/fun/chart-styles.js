// Apply the colours and widths calculated by each exhibit through CSSOM.
function renderChartMarkup(container, markup) {
  container.innerHTML = markup;
  container.querySelectorAll('[data-background], [data-color], [data-width]').forEach(function (node) {
    if (node.dataset.background) node.style.background = node.dataset.background;
    if (node.dataset.color) node.style.color = node.dataset.color;
    if (node.dataset.width) node.style.width = node.dataset.width;
  });
}
