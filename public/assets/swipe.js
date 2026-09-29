(() => {
  const card = document.querySelector('[data-swipe-card]');
  const form = document.querySelector('[data-swipe-form]');

  if (!(card instanceof HTMLElement) || !(form instanceof HTMLFormElement)) {
    return;
  }

  const buttons = {
    pass: form.querySelector('button[value="pass"]'),
    like: form.querySelector('button[value="like"]'),
  };
  let pointerId = null;
  let startX = 0;
  let startY = 0;
  let offsetX = 0;
  let dragging = false;
  let locked = false;
  let allowSubmit = false;

  const directionFor = (distance) => distance < 0 ? 'pass' : 'like';

  const paint = (distance) => {
    const width = Math.max(card.offsetWidth, 1);
    const progress = Math.min(Math.abs(distance) / (width * 0.32), 1);
    const rotation = Math.max(-13, Math.min(13, distance / 22));
    const direction = directionFor(distance);

    card.style.transform = `translate3d(${distance}px, 0, 0) rotate(${rotation}deg)`;
    card.style.setProperty('--pass-opacity', direction === 'pass' ? String(progress) : '0');
    card.style.setProperty('--like-opacity', direction === 'like' ? String(progress) : '0');
  };

  const reset = () => {
    if (locked) return;
    if (pointerId !== null && card.hasPointerCapture(pointerId)) {
      card.releasePointerCapture(pointerId);
    }
    dragging = false;
    pointerId = null;
    offsetX = 0;
    card.classList.remove('is-dragging');
    card.classList.add('is-returning');
    card.style.transform = '';
    card.style.setProperty('--pass-opacity', '0');
    card.style.setProperty('--like-opacity', '0');
    window.setTimeout(() => card.classList.remove('is-returning'), 260);
  };

  const submit = (direction) => {
    const button = buttons[direction];
    if (button instanceof HTMLButtonElement) {
      allowSubmit = true;
      form.requestSubmit(button);
    }
  };

  const commit = (direction) => {
    if (locked) return;
    locked = true;
    dragging = false;
    card.classList.remove('is-dragging', 'is-returning');
    card.classList.add('is-committing');
    card.setAttribute('aria-busy', 'true');

    const sign = direction === 'like' ? 1 : -1;
    const destination = Math.max(window.innerWidth, card.offsetWidth * 2) * sign;
    card.style.setProperty('--pass-opacity', direction === 'pass' ? '1' : '0');
    card.style.setProperty('--like-opacity', direction === 'like' ? '1' : '0');
    card.style.transform = `translate3d(${destination}px, -24px, 0) rotate(${sign * 18}deg)`;

    window.setTimeout(() => submit(direction), 260);
  };

  card.addEventListener('pointerdown', (event) => {
    if (locked || !event.isPrimary || event.button !== 0) return;
    if (event.target instanceof Element && event.target.closest('button, a, input, video')) return;

    pointerId = event.pointerId;
    startX = event.clientX;
    startY = event.clientY;
    offsetX = 0;
    dragging = true;
    card.classList.remove('is-returning');
    card.classList.add('is-dragging');
    card.setPointerCapture(event.pointerId);
  });

  card.addEventListener('pointermove', (event) => {
    if (!dragging || event.pointerId !== pointerId) return;

    const distanceX = event.clientX - startX;
    const distanceY = event.clientY - startY;
    if (Math.abs(distanceY) > Math.abs(distanceX) && Math.abs(distanceY) > 12) {
      reset();
      return;
    }

    offsetX = distanceX;
    paint(offsetX);
  });

  const release = (event) => {
    if (!dragging || event.pointerId !== pointerId) return;
    if (card.hasPointerCapture(event.pointerId)) card.releasePointerCapture(event.pointerId);

    const threshold = Math.min(110, card.offsetWidth * 0.24);
    if (Math.abs(offsetX) >= threshold) {
      commit(directionFor(offsetX));
    } else {
      reset();
    }
  };

  card.addEventListener('pointerup', release);
  card.addEventListener('pointercancel', () => {
    if (dragging) reset();
  });
  card.addEventListener('dragstart', (event) => event.preventDefault());

  form.addEventListener('submit', (event) => {
    if (allowSubmit) return;

    const submitter = event.submitter;
    if (!(submitter instanceof HTMLButtonElement)) return;
    const direction = submitter.value;
    if (direction !== 'pass' && direction !== 'like') return;

    event.preventDefault();
    commit(direction);
  });

  document.addEventListener('keydown', (event) => {
    if (locked || event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.target instanceof Element && event.target.closest('input, textarea, select, button, a')) return;

    if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
      event.preventDefault();
      commit(event.key === 'ArrowLeft' ? 'pass' : 'like');
    }
  });
})();
