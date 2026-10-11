(function () {
  'use strict';

  function chooseChannel(channels, previous) {
    const authorized = new Set(channels.map(channel => channel.id));
    if (typeof previous === 'string' && authorized.has(previous)) {
      return previous;
    }
    if (authorized.has('staff') || channels.length === 0) {
      return 'staff';
    }
    return '';
  }

  if (typeof module !== 'undefined' && module.exports) {
    module.exports = { chooseChannel };
  }
  if (typeof window !== 'undefined') {
    window.chooseChannel = chooseChannel;
  }
})();
