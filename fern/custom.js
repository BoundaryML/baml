// Immediately execute function
(function() {
  // Only create chat bubble if window width > 700px
  if (window.innerWidth <= 700) {
    console.log('Window width <= 700px, not showing chat bubble');
    return;
  }

  // Create the chat bubble
  const chatBubble = document.createElement('div');
  chatBubble.style.position = 'fixed';
  chatBubble.style.bottom = '20px';
  chatBubble.style.right = '20px';
  chatBubble.style.width = '60px';
  chatBubble.style.height = '60px';
  chatBubble.style.backgroundColor = '#6025d1';
  chatBubble.style.borderRadius = '50%';
  chatBubble.style.boxShadow = '0 4px 8px rgba(0, 0, 0, 0.2)';
  chatBubble.style.cursor = 'pointer';
  chatBubble.style.zIndex = '1000';
  chatBubble.style.transition = 'all 0.3s ease';
  chatBubble.style.display = 'flex';
  chatBubble.style.alignItems = 'center';
  chatBubble.style.justifyContent = 'center';
  
  // Add magical neon border with animation
  chatBubble.style.border = '2px solid transparent';
  chatBubble.style.backgroundClip = 'padding-box';
  
  // Create a pseudo-element for the animated border
  const borderAnimation = document.createElement('div');
  borderAnimation.style.position = 'absolute';
  borderAnimation.style.top = '-4px';
  borderAnimation.style.left = '-4px';
  borderAnimation.style.right = '-4px';
  borderAnimation.style.bottom = '-4px';
  borderAnimation.style.borderRadius = '50%';
  borderAnimation.style.zIndex = '-1';
  borderAnimation.style.background = 'linear-gradient(45deg, #ff00cc, #6025d1, #00ccff, #6025d1)';
  borderAnimation.style.backgroundSize = '200% 200%';
  borderAnimation.style.filter = 'blur(2px)';
  borderAnimation.style.opacity = '0.7';
  borderAnimation.style.transition = 'opacity 0.3s ease';
  
  // Add animation using CSS keyframes
  const style = document.createElement('style');
  style.textContent = `
    @keyframes magicBorder {
      0% { background-position: 0% 50%; }
      50% { background-position: 100% 50%; }
      100% { background-position: 0% 50%; }
    }
  `;
  document.head.appendChild(style);
  
  borderAnimation.style.animation = 'magicBorder 20s ease infinite';
  chatBubble.appendChild(borderAnimation);
  
  // Add chat icon
  const chatIcon = document.createElement('div');
  chatIcon.innerHTML = `
  <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-bot"><path d="M12 8V4H8"/><rect width="16" height="12" x="4" y="8" rx="2"/><path d="M2 14h2"/><path d="M20 14h2"/><path d="M15 13v2"/><path d="M9 13v2"/></svg>
  `;



  chatBubble.appendChild(chatIcon);

  // Create tooltip
  const tooltip = document.createElement('div');
  tooltip.textContent = 'Ask BAML AI Agent';
  tooltip.style.position = 'absolute';
  tooltip.style.right = '70px';
  tooltip.style.backgroundColor = 'rgba(0, 0, 0, 0.7)';
  tooltip.style.color = 'white';
  tooltip.style.padding = '8px 12px';
  tooltip.style.borderRadius = '4px';
  tooltip.style.fontSize = '14px';
  tooltip.style.whiteSpace = 'nowrap';
  tooltip.style.opacity = '0';
  tooltip.style.visibility = 'hidden';
  tooltip.style.transition = 'opacity 0.3s, visibility 0.3s';
  chatBubble.appendChild(tooltip);
  
  // Create sparkle container
  const sparkleContainer = document.createElement('div');
  sparkleContainer.style.position = 'absolute';
  sparkleContainer.style.width = '40px';
  sparkleContainer.style.height = '40px';
  sparkleContainer.style.top = '5px';
  sparkleContainer.style.left = '5px';
  sparkleContainer.style.pointerEvents = 'none';
  sparkleContainer.style.opacity = '0';
  sparkleContainer.style.transition = 'opacity 0.3s';
  chatBubble.appendChild(sparkleContainer);
  
  // Function to create a sparkle
  function createSparkle() {
    const sparkle = document.createElement('div');
    sparkle.style.position = 'absolute';
    sparkle.style.width = '3px';
    sparkle.style.height = '3px';
    sparkle.style.borderRadius = '30%';
    sparkle.style.backgroundColor = '#ffffff';
    sparkle.style.boxShadow = '0 0 5px 1px rgba(255, 255, 255, 0.8)';
    
    // Random position
    sparkle.style.left = Math.random() * 100 + '%';
    sparkle.style.top = Math.random() * 100 + '%';
    
    // Animation
    sparkle.animate([
      { transform: 'scale(0)', opacity: 0 },
      { transform: 'scale(1)', opacity: 1 },
      { transform: 'scale(0)', opacity: 0 }
    ], {
      duration: 1200 + Math.random() * 800,
      easing: 'ease-out'
    });
    
    sparkleContainer.appendChild(sparkle);
    
    // Remove after animation
    setTimeout(() => {
      sparkle.remove();
    }, 1000);
  }
  
  // Sparkle interval reference
  let sparkleInterval;
  
  // Add hover effects
  chatBubble.addEventListener('mouseover', function() {
    chatBubble.style.backgroundColor = '#6025d1';
    chatBubble.style.transform = 'scale(1.05)';
    tooltip.style.opacity = '1';
    tooltip.style.visibility = 'visible';
    
    // Enhance the magical border on hover
    borderAnimation.style.opacity = '1';
    borderAnimation.style.filter = 'blur(4px)';
    
    // Show sparkle container
    sparkleContainer.style.opacity = '1';
    
    // Create sparkles periodically
    sparkleInterval = setInterval(createSparkle, 300);
  });
  
  chatBubble.addEventListener('mouseout', function() {
    chatBubble.style.backgroundColor = '#6025d1';
    chatBubble.style.transform = 'scale(1)';
    tooltip.style.opacity = '0';
    tooltip.style.visibility = 'hidden';
    
    // Reduce the magical border effect when not hovering
    borderAnimation.style.opacity = '0.7';
    borderAnimation.style.filter = 'blur(5px)';
    
    // Hide sparkle container
    sparkleContainer.style.opacity = '0';
    
    // Stop creating new sparkles
    clearInterval(sparkleInterval);
  });
  
  // Add click event
  chatBubble.addEventListener('click', function() {
    window.open('https://boundaryml.com/chat', '_blank', "noopener,noreferrer");
  });
  
  // Function to append to body when it's available
  function appendToBody() {
    if (document.body) {
      document.body.appendChild(chatBubble);
      console.log('Chat bubble added to DOM');
    } else {
      // If body isn't available yet, try again shortly
      setTimeout(appendToBody, 50);
    }
  }
  
  // Start the process
  appendToBody();
  
  // Also listen for window resize to hide/show based on width
  window.addEventListener('resize', function() {
    if (window.innerWidth <= 700) {
      chatBubble.style.display = 'none';
    } else {
      chatBubble.style.display = 'flex';
    }
  });
})();
