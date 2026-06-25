import React, { createContext, useContext, useState, useCallback, ReactNode, useEffect, useRef, useLayoutEffect } from 'react';
import { CheckCircle2, AlertCircle, Info, XCircle, X } from 'lucide-react';

export type ToastVariant = 'success' | 'info' | 'warning' | 'error';

export interface ToastMessage {
  id: string;
  message: React.ReactNode;
  variant?: ToastVariant;
  duration?: number;
  isActionable?: boolean;
}

interface ToastState extends ToastMessage {
  exiting?: boolean;
}

interface ToastContextType {
  addToast: (toast: Omit<ToastMessage, 'id'>) => void;
  removeToast: (id: string) => void;
}

const ToastContext = createContext<ToastContextType | undefined>(undefined);

export const ToastProvider: React.FC<{ children: ReactNode }> = ({ children }) => {
  const [toasts, setToasts] = useState<ToastState[]>([]);
  const nextToastId = useRef(0);

  const addToast = useCallback((toast: Omit<ToastMessage, 'id'>) => {
    nextToastId.current += 1;
    setToasts(prev => [...prev, { ...toast, id: `toast-${nextToastId.current}` }]);
  }, []);

  const removeToast = useCallback((id: string) => {
    setToasts(prev => {
      // Prevent multiple exit calls and re-renders for the same ID
      if (prev.find(t => t.id === id)?.exiting) return prev;
      return prev.map(t => t.id === id ? { ...t, exiting: true } : t);
    });
  }, []);

  const removeToastCompletely = useCallback((id: string) => {
    setToasts(prev => prev.filter(t => t.id !== id));
  }, []);

  return (
    <ToastContext.Provider value={{ addToast, removeToast }}>
      {children}
      <ToastContainer toasts={toasts} removeToast={removeToast} removeToastCompletely={removeToastCompletely} />
    </ToastContext.Provider>
  );
};

export const useToast = () => {
  const context = useContext(ToastContext);
  if (!context) throw new Error('useToast must be used within ToastProvider');
  return context;
};

const ToastItem: React.FC<{ toast: ToastState; removeToast: (id: string) => void; removeToastCompletely: (id: string) => void }> = ({ toast, removeToast, removeToastCompletely }) => {
  const [isMounted, setIsMounted] = useState(false);
  const [isHovered, setIsHovered] = useState(false);
  const elementRef = useRef<HTMLDivElement>(null);
  const [contentHeight, setContentHeight] = useState<number | undefined>(undefined);

  useLayoutEffect(() => {
    if (elementRef.current && !isMounted) {
      // Measure natural height of the toast
      setContentHeight(elementRef.current.offsetHeight);
      
      let frame2: number;
      const frame1 = requestAnimationFrame(() => {
        frame2 = requestAnimationFrame(() => setIsMounted(true));
      });
      return () => {
        cancelAnimationFrame(frame1);
        if (frame2) cancelAnimationFrame(frame2);
      };
    }
  }, [isMounted]);

  useEffect(() => {
    // 1. If exiting, don't trigger the auto-dismiss timer
    if (toast.exiting) return;

    // Explicitly treat a duration of 0 as a permanent toast
    if (toast.duration === 0 || toast.isActionable || (toast.variant === 'error' && !toast.duration)) {
      return;
    }

    let timeoutDuration = toast.duration ?? 5000;
    if (timeoutDuration < 5000) timeoutDuration = 5000;

    if (isHovered) {
      return;
    }

    const timer = setTimeout(() => {
      removeToast(toast.id);
    }, timeoutDuration);

    return () => clearTimeout(timer);
  }, [toast, isHovered, removeToast]);

  // Fallback safety timer just in case browser drops onTransitionEnd
  useEffect(() => {
    if (toast.exiting) {
      const fallbackTimer = setTimeout(() => {
        removeToastCompletely(toast.id);
      }, 500); 
      return () => clearTimeout(fallbackTimer);
    }
  }, [toast.exiting, toast.id, removeToastCompletely]);

  const role = toast.variant === 'error' ? 'alert' : 'status';

  const variantStyles = {
    success: 'border-emerald-500/30 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 shadow-emerald-500/10',
    info: 'border-blue-500/30 bg-blue-500/10 text-blue-600 dark:text-blue-400 shadow-blue-500/10',
    warning: 'border-amber-500/30 bg-amber-500/10 text-amber-600 dark:text-amber-400 shadow-amber-500/10',
    error: 'border-red-500/30 bg-red-500/10 text-red-600 dark:text-red-400 shadow-red-500/10',
  };

  const icons = {
    success: <CheckCircle2 className="w-5 h-5 shrink-0" strokeWidth={2.5} />,
    error: <XCircle className="w-5 h-5 shrink-0" strokeWidth={2.5} />,
    warning: <AlertCircle className="w-5 h-5 shrink-0" strokeWidth={2.5} />,
    info: <Info className="w-5 h-5 shrink-0" strokeWidth={2.5} />,
  };

  const variant = toast.variant || 'info';
  const style = variantStyles[variant];
  const Icon = icons[variant];

  const isVisible = isMounted && !toast.exiting;

  return (
    <div
      className={`w-full transition-all duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] flex flex-col justify-end ${isVisible ? 'mb-3' : 'mb-0'}`}
      style={{
        height: isVisible && contentHeight !== undefined ? contentHeight : 0,
      }}
      onTransitionEnd={(e) => {
        // 2. Rely on height transition end to safely unmount, instead of hardcoded fixed setTimeouts
        if (toast.exiting && e.target === e.currentTarget && e.propertyName === 'height') {
          removeToastCompletely(toast.id);
        }
      }}
    >
      <div
        ref={elementRef}
        role={role}
        className={`app-toast-item shrink-0 ${isVisible ? 'pointer-events-auto' : 'pointer-events-none'} flex items-start gap-3 rounded-[16px] border px-4 py-3 shadow-[0_8px_30px_rgb(0,0,0,0.12),0_0_20px_var(--tw-shadow-color)] backdrop-blur-xl text-[14px] leading-relaxed transition-all duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] ${style}`}
        style={{
          opacity: isVisible ? 1 : 0,
          transform: isVisible ? 'translateY(0) scale(1)' : 'translateY(24px) scale(0.95)',
          transformOrigin: 'bottom center',
        }}
        onMouseEnter={() => setIsHovered(true)}
        onMouseLeave={() => setIsHovered(false)}
        onFocus={() => setIsHovered(true)}
        onBlur={() => setIsHovered(false)}
      >
        <div className="mt-0.5 shrink-0">{Icon}</div>
        <div className="font-semibold flex-1 tracking-tight break-all whitespace-pre-wrap">{toast.message}</div>
        <button
          onClick={() => removeToast(toast.id)}
          className="shrink-0 ml-2 mt-0.5 opacity-60 hover:opacity-100 hover:bg-black/5 dark:hover:bg-white/10 p-1 rounded-full transition-all active:scale-90"
          aria-label="Dismiss notification"
        >
          <X className="w-4 h-4" strokeWidth={2.5} />
        </button>
      </div>
    </div>
  );
};

const ToastContainer: React.FC<{ toasts: ToastState[]; removeToast: (id: string) => void; removeToastCompletely: (id: string) => void }> = ({ toasts, removeToast, removeToastCompletely }) => {
  return (
    // 3. Removed 'gap-3' to allow the dynamic mb-3 from the wrapper to handle spacing, allowing it to smoothly collapse
    <div className="fixed bottom-8 left-1/2 -translate-x-1/2 z-[100] flex w-full max-w-[420px] flex-col pointer-events-none items-center px-4">
      {toasts.map(toast => (
        <ToastItem key={toast.id} toast={toast} removeToast={removeToast} removeToastCompletely={removeToastCompletely} />
      ))}
    </div>
  );
};
