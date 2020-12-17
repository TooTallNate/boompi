import createDebug from 'debug';
import React, { useState, useEffect, useCallback, useRef } from 'react';

const debug = createDebug('boompi:components:marquee');

// CSS
import styles from '@styles/marquee.module.css';

export default function Marquee({ children }: any) {
	const content = useRef();
	const outer = useRef();
	const [width, setWidth] = useState(0);

	useEffect(() => {
		const { width: outerWidth } = outer.current.getBoundingClientRect();
		const { width: contentWidth } = content.current.getBoundingClientRect();
		setWidth(contentWidth <= outerWidth ? 0 : contentWidth);
	}, [children]);

	const innerStyle: CSSStyleDeclaration = {};
	const outerClasses = [styles.outer];
	const innerClasses = [styles.inner];
	const spacerWidth = 200;

	let runningComponents = null;
	if (width > 0) {
		const copy = React.Children.map(children, (el) => {
			if (React.isValidElement(el)) {
				return React.cloneElement(el);
			}
			return el;
		});
		runningComponents = (
			<>
				<div
					style={{
						width: `${spacerWidth}px`,
						display: 'inline-block',
					}}
				/>
				<span className={styles.copy}>{copy}</span>
			</>
		);
		innerStyle.transform = `translateX(-${width + spacerWidth}px)`;
		innerClasses.push(styles.active);
		outerClasses.push(styles.outerActive);
	}

	return (
		<div ref={outer} className={outerClasses.join(' ')}>
			<div className={innerClasses.join(' ')} style={innerStyle}>
				<span ref={content} className={styles.source}>
					{children}
				</span>
				{runningComponents}
			</div>
		</div>
	);
}
